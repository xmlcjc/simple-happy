#!/usr/bin/env node
// Builds the Windows desktop packages:
//   frontend -> release binary -> NSIS installer + portable zip
//
// The frontend assets are embedded into the exe at compile time by
// `tauri::generate_context!` (frontendDist), so the portable folder is just the
// executable:
//   Simple Happy.exe
//   data/          <- created on first launch, next to the exe
//
// Outputs (in desktop/dist/):
//   <productName>_<ver>_x64-setup.exe   NSIS installer
//   simple-happy-portable-v<ver>-win-x64.zip
//
// The installer filename is derived from the resolved `productName`, which the
// Windows build overrides in tauri.windows.conf.json (see readProductName).
//
// NOTE: cargo release builds must be low-concurrency (`-j 4`) because Windows
// antivirus real-time scanning races cargo's parallel .rlib writes and causes
// random "invalid metadata" / link.exe failures. We set CARGO_BUILD_JOBS=4 so
// the `tauri build` step inherits it.

import { execFileSync, execSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const frontendDir = join(repoRoot, 'frontend')
const desktopDir = join(repoRoot, 'desktop')
const tauriDir = join(desktopDir, 'src-tauri')
const outRoot = join(desktopDir, 'dist')
const stageDir = join(outRoot, 'Simple Happy')
// tauri build resolves beforeBuildCommand's relative path (../../frontend)
// against the CWD, so it must run from src-tauri/ (where ../../frontend is
// correct). Invoke the CLI by absolute path to avoid npx re-resolving.
const tauriCli = join(desktopDir, 'node_modules', '@tauri-apps', 'cli', 'tauri.js')

function run(command, args, cwd = repoRoot, env = {}) {
  console.log(`\n> ${command} ${args.join(' ')}`)
  execFileSync(command, args, { cwd, stdio: 'inherit', env: { ...process.env, ...env } })
}

/// For commands that need a shell to resolve (npm is npm.cmd on Windows).
function runShell(command, cwd = repoRoot, env = {}) {
  console.log(`\n> ${command}`)
  execSync(command, { cwd, stdio: 'inherit', env: { ...process.env, ...env } })
}

function readVersion() {
  const cargoToml = readFileSync(join(repoRoot, 'Cargo.toml'), 'utf8')
  const match = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)
  if (!match) throw new Error('无法从 Cargo.toml 读取版本号')
  return match[1]
}

/// The NSIS installer is named `<productName>_<version>_x64-setup.exe`, so the
/// name has to be read from the config rather than hardcoded: the Windows build
/// overrides `productName` in tauri.windows.conf.json (it must stay unique in
/// the Windows uninstall registry, see desktop/src-tauri/windows-hooks.nsh).
function readProductName() {
  const readJson = (file) =>
    JSON.parse(readFileSync(join(tauriDir, file), 'utf8').replace(/^\uFEFF/, ''))
  const base = readJson('tauri.conf.json')
  const windowsConf = join(tauriDir, 'tauri.windows.conf.json')
  const override = existsSync(windowsConf) ? readJson('tauri.windows.conf.json') : {}
  const productName = override.productName ?? base.productName
  if (!productName) throw new Error('无法从 tauri.conf.json 读取 productName')
  return productName
}

const version = readVersion()
const productName = readProductName()
// 版本号分散在 7 个文件里（安装包名与包内版本都由它们决定）。本地打包前先跑一遍
// 与 CI 相同的校验，避免手工 bump 时漏改某个文件 —— 装出来才发现版本不符。
run('node', [join(repoRoot, 'scripts', 'check-release-metadata.mjs')])
console.log(`打包 Simple Happy 桌面版 v${version}（productName: ${productName}）`)

// `ci` rather than `install` so a release build can never drift the lockfile.
runShell('npm ci', frontendDir)
// Build the frontend here (beforeBuildCommand is a no-op) so the relative
// path in tauri.conf.json never has to resolve against tauri's CWD.
runShell('npm run build', frontendDir)

// `tauri build` runs cargo release build + NSIS bundling. CARGO_BUILD_JOBS=4
// keeps the antivirus race at bay. Run from src-tauri/ so the relative
// frontendDist path resolves correctly.
run('node', [tauriCli, 'build'], tauriDir, { CARGO_BUILD_JOBS: '4' })

// ── NSIS installer ──
// The workspace shares the repo-root target/ dir, so the bundle lands under
// <repoRoot>/target/release/bundle/nsis, not desktop/src-tauri/target.
const nsisDir = join(repoRoot, 'target', 'release', 'bundle', 'nsis')
const nsisName = `${productName}_${version}_x64-setup.exe`
const nsisSrc = join(nsisDir, nsisName)
if (!existsSync(nsisSrc)) {
  throw new Error(`未找到 NSIS 安装包: ${nsisSrc}`)
}
const nsisOut = join(outRoot, nsisName)
rmSync(nsisOut, { force: true })
cpSync(nsisSrc, nsisOut)
console.log(`\nNSIS 安装包: ${nsisOut}`)

// ── Portable zip ──
const exeSrc = join(repoRoot, 'target', 'release', 'reader-desktop.exe')
if (!existsSync(exeSrc)) {
  throw new Error(`未找到编译产物: ${exeSrc}`)
}

rmSync(stageDir, { recursive: true, force: true })
mkdirSync(stageDir, { recursive: true })
cpSync(exeSrc, join(stageDir, 'Simple Happy.exe'))
writeFileSync(join(stageDir, '.simple-happy-portable'), 'Simple Happy portable package\n')
// 便携版也要随包带上内置音源：安装版由 Tauri resources 复制，
// 便携包只有 exe + data，缺少这个目录会导致首启装不出任何音源。
const builtinPlugins = join(tauriDir, 'builtin-music-plugins')
if (existsSync(builtinPlugins)) {
  cpSync(builtinPlugins, join(stageDir, 'builtin-music-plugins'), { recursive: true })
  console.log('已加入内置音源: builtin-music-plugins')
}
// 便携版预置书源/RSS 种子：首次启动即带本地书源与订阅源。
// 安装版仍由用户在应用内导入，避免把第三方书源写进系统目录。
const seedDb = join(repoRoot, 'seed', 'reader.db')
rmSync(join(stageDir, 'data'), { recursive: true, force: true })
if (existsSync(seedDb)) {
  mkdirSync(join(stageDir, 'data'), { recursive: true })
  cpSync(seedDb, join(stageDir, 'data', 'reader.db'))
  console.log('已加入书源种子: seed/reader.db')
}

const zipPath = join(outRoot, `simple-happy-portable-v${version}-win-x64.zip`)
rmSync(zipPath, { force: true })
run('tar.exe', ['-a', '-c', '-f', zipPath, '-C', stageDir, '.'])

console.log(`\n完成:\n  安装包 ${nsisOut}\n  压缩包 ${zipPath}`)
