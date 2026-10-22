//! MusicFree 插件运行时垫片。
//!
//! QuickJS 没有浏览器/Node 的全局对象, 而插件(尤其是用 webpack 打包过的)
//! 默认自己跑在浏览器或 Electron 渲染进程里。这里提供一套最小可用垫片:
//! 网络(fetch/XHR)、编码(TextEncoder/TextDecoder/Buffer/atob/btoa)、
//! URL/URLSearchParams、定时器与 console。
//!
//! 网络出口由宿主注入的 `__mf_http(payload) -> json` 提供。它是**同步**调用,
//! 因此插件里 `await fetch(...)` 实际会在当前微任务里立即完成 —— 这让我们
//! 可以在一个线程内同步驱动插件, 不必引入完整的异步宿主。
//!
//! 明确不支持: Node 内置模块(fs/path/child_process 等)。`require` 会抛出
//! 带模块名的错误, 由上层翻译成用户可读的提示。

pub const PRELUDE: &str = r##"
(function () {
  var g = globalThis;
  g.global = g;
  g.window = g;
  g.self = g;
  g.process = g.process || { env: {}, platform: 'win32', versions: {} };

  function noop() {}
  if (!g.console) {
    g.console = { log: noop, warn: noop, error: noop, info: noop, debug: noop };
  }

  // 定时器不能直接当 0ms 微任务执行：插件常用
  // `Promise.race([真实请求, 超时定时器])` 做取链竞争，若超时回调先跑，
  // 所有源都会被判失败。这里只登记回调，由宿主在微任务耗尽后逐个执行，
  // 效果等价于「请求先完成，超时后到」。
  var timers = [];
  g.setTimeout = function (fn) {
    if (typeof fn === 'function') timers.push(fn);
    return timers.length;
  };
  g.clearTimeout = noop;
  g.setInterval = function () { return 0; };
  g.clearInterval = noop;
  g.__mf_run_timer = function () {
    if (!timers.length) return false;
    var fn = timers.shift();
    try { fn(); } catch (e) {}
    return true;
  };

  // ── UTF-8 ──
  function utf8Encode(str) {
    var bytes = [];
    for (var i = 0; i < str.length; i++) {
      var c = str.charCodeAt(i);
      if (c < 0x80) {
        bytes.push(c);
      } else if (c < 0x800) {
        bytes.push(0xc0 | (c >> 6), 0x80 | (c & 63));
      } else if (c >= 0xd800 && c <= 0xdbff && i + 1 < str.length) {
        var c2 = str.charCodeAt(++i);
        var cp = 0x10000 + ((c - 0xd800) << 10) + (c2 - 0xdc00);
        bytes.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 63), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
      } else {
        bytes.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      }
    }
    return new Uint8Array(bytes);
  }

  function utf8Decode(bytes) {
    var out = '';
    for (var i = 0; i < bytes.length; ) {
      var b = bytes[i++];
      var cp;
      if (b < 0x80) {
        cp = b;
      } else if (b >= 0xc0 && b < 0xe0) {
        cp = ((b & 31) << 6) | (bytes[i++] & 63);
      } else if (b >= 0xe0 && b < 0xf0) {
        cp = ((b & 15) << 12) | ((bytes[i++] & 63) << 6) | (bytes[i++] & 63);
      } else {
        cp = ((b & 7) << 18) | ((bytes[i++] & 63) << 12) | ((bytes[i++] & 63) << 6) | (bytes[i++] & 63);
      }
      if (cp > 0xffff) {
        cp -= 0x10000;
        out += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 1023));
      } else {
        out += String.fromCharCode(cp);
      }
    }
    return out;
  }

  g.TextEncoder = function () {};
  g.TextEncoder.prototype.encode = function (str) { return utf8Encode(String(str)); };
  g.TextDecoder = function () {};
  g.TextDecoder.prototype.decode = function (bytes) {
    if (!bytes) return '';
    return utf8Decode(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes));
  };

  // ── Base64 ──
  var B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  function base64Encode(bytes) {
    var out = '';
    for (var i = 0; i < bytes.length; i += 3) {
      var a = bytes[i], b = bytes[i + 1], c = bytes[i + 2];
      out += B64[a >> 2];
      out += B64[((a & 3) << 4) | (b === undefined ? 0 : b >> 4)];
      out += b === undefined ? '=' : B64[((b & 15) << 2) | (c === undefined ? 0 : c >> 6)];
      out += c === undefined ? '=' : B64[c & 63];
    }
    return out;
  }
  function base64Decode(str) {
    var clean = String(str).replace(/[^A-Za-z0-9+/=]/g, '');
    var out = [];
    for (var i = 0; i < clean.length; i += 4) {
      var a = B64.indexOf(clean[i]);
      var b = B64.indexOf(clean[i + 1]);
      var c = B64.indexOf(clean[i + 2]);
      var d = B64.indexOf(clean[i + 3]);
      out.push((a << 2) | (b >> 4));
      if (c >= 0) out.push(((b & 15) << 4) | (c >> 2));
      if (d >= 0) out.push(((c & 3) << 6) | d);
    }
    return new Uint8Array(out);
  }
  g.btoa = function (str) { return base64Encode(utf8Encode(String(str))); };
  g.atob = function (str) { return utf8Decode(base64Decode(str)); };

  // ── Buffer(最小实现) ──
  function decorate(bytes) {
    bytes.__isBuffer = true;
    bytes.toString = function (enc) {
      if (enc === 'base64') return base64Encode(this);
      if (enc === 'hex') {
        var hex = '';
        for (var i = 0; i < this.length; i++) hex += (this[i] < 16 ? '0' : '') + this[i].toString(16);
        return hex;
      }
      return utf8Decode(this);
    };
    return bytes;
  }
  g.Buffer = {
    from: function (input, enc) {
      if (typeof input === 'string') {
        if (enc === 'base64') return decorate(base64Decode(input));
        if (enc === 'hex') {
          var out = [];
          for (var i = 0; i < input.length; i += 2) out.push(parseInt(input.substr(i, 2), 16) || 0);
          return decorate(new Uint8Array(out));
        }
        return decorate(utf8Encode(input));
      }
      return decorate(new Uint8Array(input || []));
    },
    alloc: function (n) { return decorate(new Uint8Array(n || 0)); },
    concat: function (list) {
      var total = 0, i;
      for (i = 0; i < list.length; i++) total += list[i].length;
      var merged = new Uint8Array(total), offset = 0;
      for (i = 0; i < list.length; i++) { merged.set(list[i], offset); offset += list[i].length; }
      return decorate(merged);
    },
    isBuffer: function (value) { return !!(value && value.__isBuffer); }
  };

  // ── URL / URLSearchParams(最小实现) ──
  function URLSearchParams(init) {
    this._map = {};
    var source = init || '';
    if (typeof source === 'object') {
      for (var key in source) this.append(key, source[key]);
    } else if (source.charAt(0) === '?') {
      source = source.slice(1);
    }
    if (typeof source === 'string' && source) {
      var parts = source.split('&');
      for (var i = 0; i < parts.length; i++) {
        if (!parts[i]) continue;
        var idx = parts[i].indexOf('=');
        var k = idx < 0 ? parts[i] : parts[i].slice(0, idx);
        var v = idx < 0 ? '' : parts[i].slice(idx + 1);
        this.append(decodeURIComponent(k), decodeURIComponent(v));
      }
    }
  }
  URLSearchParams.prototype.append = function (key, value) {
    var k = String(key);
    if (!this._map[k]) this._map[k] = [];
    this._map[k].push(String(value));
  };
  URLSearchParams.prototype.get = function (key) {
    var list = this._map[String(key)];
    return list && list.length ? list[0] : null;
  };
  URLSearchParams.prototype.getAll = function (key) { return (this._map[String(key)] || []).slice(); };
  URLSearchParams.prototype.set = function (key, value) { this._map[String(key)] = [String(value)]; };
  URLSearchParams.prototype.delete = function (key) { delete this._map[String(key)]; };
  URLSearchParams.prototype.forEach = function (fn) {
    for (var key in this._map) {
      for (var i = 0; i < this._map[key].length; i++) fn(this._map[key][i], key, this);
    }
  };
  URLSearchParams.prototype.toString = function () {
    var out = [];
    for (var key in this._map) {
      for (var i = 0; i < this._map[key].length; i++) {
        out.push(encodeURIComponent(key) + '=' + encodeURIComponent(this._map[key][i]));
      }
    }
    return out.join('&');
  };
  g.URLSearchParams = URLSearchParams;

  g.URL = function (url, base) {
    var raw = String(url);
    if (base && !/^[a-zA-Z][\w+.-]*:\/\//.test(raw)) {
      var b = String(base).replace(/\/$/, '');
      raw = raw.charAt(0) === '/' ? raw : b + '/' + raw;
    }
    var match = /^([a-zA-Z][\w+.-]*:)\/\/([^\/?#]*)([^?#]*)(\?[^#]*)?(#.*)?$/.exec(raw);
    this.href = raw;
    this.protocol = match ? match[1] : '';
    this.host = match ? match[2] : '';
    this.hostname = this.host.split(':')[0];
    this.pathname = match ? match[3] : '';
    this.search = match && match[4] ? match[4] : '';
    this.hash = match && match[5] ? match[5] : '';
    this.origin = this.protocol + '//' + this.host;
    this.searchParams = new URLSearchParams(this.search);
  };
  g.URL.prototype.toString = function () { return this.href; };

  // ── HTTP 桥 ──
  function request(payload) {
    var raw = __mf_http(JSON.stringify(payload));
    var res;
    try { res = JSON.parse(raw); } catch (e) { return { error: 'HTTP 桥返回了无效数据' }; }
    return res;
  }

  function lowerHeaders(list) {
    var map = {};
    if (!list) return map;
    for (var key in list) map[String(key).toLowerCase()] = String(list[key]);
    return map;
  }

  g.fetch = function (url, options) {
    options = options || {};
    var headers = {};
    if (options.headers) {
      if (typeof options.headers.forEach === 'function' && !Array.isArray(options.headers)) {
        options.headers.forEach(function (value, key) { headers[key] = value; });
      } else {
        for (var key in options.headers) headers[key] = options.headers[key];
      }
    }
    var res = request({
      url: String(url),
      method: String(options.method || 'GET').toUpperCase(),
      headers: headers,
      body: options.body == null ? null : String(options.body)
    });
    if (res.error) return Promise.reject(new Error(res.error));
    var headerMap = lowerHeaders(res.headers);
    return Promise.resolve({
      ok: res.status >= 200 && res.status < 300,
      status: res.status,
      statusText: '',
      url: res.url || String(url),
      headers: { get: function (name) { return headerMap[String(name).toLowerCase()] || null; } },
      text: function () { return Promise.resolve(res.body || ''); },
      json: function () {
        try { return Promise.resolve(JSON.parse(res.body || 'null')); }
        catch (e) { return Promise.reject(e); }
      },
      arrayBuffer: function () { return Promise.resolve(utf8Encode(res.body || '').buffer); }
    });
  };

  g.XMLHttpRequest = function () {
    this.readyState = 0;
    this.status = 0;
    this.responseText = '';
    this.response = '';
    this.onload = null;
    this.onerror = null;
    this.onreadystatechange = null;
    this.onloadend = null;
    this.timeout = 0;
    this.withCredentials = false;
    this._headers = {};
  };
  g.XMLHttpRequest.prototype.open = function (method, url) {
    this._method = String(method || 'GET').toUpperCase();
    this._url = String(url);
    this.readyState = 1;
    if (typeof this.onreadystatechange === 'function') this.onreadystatechange();
  };
  g.XMLHttpRequest.prototype.setRequestHeader = function (key, value) {
    this._headers[String(key)] = String(value);
  };
  g.XMLHttpRequest.prototype.getAllResponseHeaders = function () {
    var out = '';
    for (var key in (this._responseHeaders || {})) out += key + ': ' + this._responseHeaders[key] + '\r\n';
    return out;
  };
  g.XMLHttpRequest.prototype.getResponseHeader = function (name) {
    return (this._responseHeaders || {})[String(name).toLowerCase()] || null;
  };
  g.XMLHttpRequest.prototype.send = function (body) {
    var res = request({
      url: this._url,
      method: this._method,
      headers: this._headers,
      body: body == null ? null : String(body)
    });
    this._responseHeaders = lowerHeaders(res.headers);
    if (res.error) {
      this.readyState = 4;
      this.status = 0;
      if (typeof this.onreadystatechange === 'function') this.onreadystatechange();
      if (typeof this.onerror === 'function') this.onerror(new Error(res.error));
      if (typeof this.onloadend === 'function') this.onloadend();
      return;
    }
    this.readyState = 4;
    this.status = res.status;
    this.responseURL = res.url || this._url;
    this.responseText = res.body || '';
    this.response = this.responseText;
    if (typeof this.onreadystatechange === 'function') this.onreadystatechange();
    if (typeof this.onload === 'function') this.onload();
    if (typeof this.onloadend === 'function') this.onloadend();
  };
  g.XMLHttpRequest.prototype.abort = noop;

  // ── 内置模块 ──
  // MusicFree 通过 require 向插件注入 axios / crypto-js / qs / he / dayjs /
  // big-integer 等常用库。第三方插件几乎都依赖它们，缺一个就整包不可用。
  // 这里提供行为等价的精简实现，未覆盖的 API 会显式报错，便于定位。
  var packages = {};
  g.__mf_packages = packages;

  function toBytes(text) {
    return Array.prototype.slice.call(utf8Encode(String(text)));
  }

  function bytesToHex(bytes) {
    var out = '';
    for (var i = 0; i < bytes.length; i++) {
      var part = bytes[i].toString(16);
      out += part.length === 1 ? '0' + part : part;
    }
    return out;
  }

  function hexToBytes(text) {
    var out = [];
    for (var i = 0; i < text.length; i += 2) out.push(parseInt(text.substr(i, 2), 16));
    return out;
  }

  // ── MD5 ──
  function md5Hex(message) {
    var S = [7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
             5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
             4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
             6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21];
    var K = [];
    for (var k = 0; k < 64; k++) K[k] = Math.floor(Math.abs(Math.sin(k + 1)) * 4294967296) >>> 0;

    var bytes = toBytes(message);
    var bitLength = bytes.length * 8;
    bytes.push(0x80);
    while (bytes.length % 64 !== 56) bytes.push(0);
    var high = Math.floor(bitLength / 4294967296);
    var low = bitLength >>> 0;
    for (var i = 0; i < 4; i++) bytes.push((low >>> (i * 8)) & 0xff);
    for (var i2 = 0; i2 < 4; i2++) bytes.push((high >>> (i2 * 8)) & 0xff);

    var state = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    for (var offset = 0; offset < bytes.length; offset += 64) {
      var words = [];
      for (var w = 0; w < 16; w++) {
        words[w] = (bytes[offset + w * 4] | (bytes[offset + w * 4 + 1] << 8) |
          (bytes[offset + w * 4 + 2] << 16) | (bytes[offset + w * 4 + 3] << 24)) >>> 0;
      }
      var a = state[0], b = state[1], c = state[2], d = state[3];
      for (var step = 0; step < 64; step++) {
        var f, g;
        if (step < 16) { f = (b & c) | (~b & d); g = step; }
        else if (step < 32) { f = (d & b) | (~d & c); g = (5 * step + 1) % 16; }
        else if (step < 48) { f = b ^ c ^ d; g = (3 * step + 5) % 16; }
        else { f = c ^ (b | ~d); g = (7 * step) % 16; }
        f = (f + a + K[step] + words[g]) >>> 0;
        a = d;
        d = c;
        c = b;
        var rotate = S[step];
        b = (b + ((f << rotate) | (f >>> (32 - rotate)))) >>> 0;
      }
      state[0] = (state[0] + a) >>> 0;
      state[1] = (state[1] + b) >>> 0;
      state[2] = (state[2] + c) >>> 0;
      state[3] = (state[3] + d) >>> 0;
    }

    var result = '';
    for (var index = 0; index < 4; index++) {
      for (var byteIndex = 0; byteIndex < 4; byteIndex++) {
        result += ((state[index] >>> (byteIndex * 8)) & 0xff).toString(16).padStart(2, '0');
      }
    }
    return result;
  }

  var SHA256_K = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
  ];

  function sha256Hex(message) {
    return sha256HexBytes(toBytes(message));
  }

  /// 对原始字节做 SHA-256（HMAC 需要对二进制拼接结果求哈希）。
  function sha256HexBytes(input) {
    var bytes = input.slice();
    var bitLength = bytes.length * 8;
    bytes.push(0x80);
    while (bytes.length % 64 !== 56) bytes.push(0);
    var high = Math.floor(bitLength / 4294967296);
    var low = bitLength >>> 0;
    bytes.push((high >>> 24) & 0xff, (high >>> 16) & 0xff, (high >>> 8) & 0xff, high & 0xff);
    bytes.push((low >>> 24) & 0xff, (low >>> 16) & 0xff, (low >>> 8) & 0xff, low & 0xff);

    var hash = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    var words = new Array(64);

    function rotr(value, bits) {
      return (value >>> bits) | (value << (32 - bits));
    }

    for (var offset = 0; offset < bytes.length; offset += 64) {
      for (var i = 0; i < 16; i++) {
        words[i] = ((bytes[offset + i * 4] << 24) | (bytes[offset + i * 4 + 1] << 16) |
          (bytes[offset + i * 4 + 2] << 8) | bytes[offset + i * 4 + 3]) >>> 0;
      }
      for (var j = 16; j < 64; j++) {
        var s0 = rotr(words[j - 15], 7) ^ rotr(words[j - 15], 18) ^ (words[j - 15] >>> 3);
        var s1 = rotr(words[j - 2], 17) ^ rotr(words[j - 2], 19) ^ (words[j - 2] >>> 10);
        words[j] = (words[j - 16] + s0 + words[j - 7] + s1) >>> 0;
      }
      var a = hash[0], b = hash[1], c = hash[2], d = hash[3];
      var e = hash[4], f = hash[5], g = hash[6], h = hash[7];
      for (var step = 0; step < 64; step++) {
        var big1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
        var choose = (e & f) ^ (~e & g);
        var temp1 = (h + big1 + choose + SHA256_K[step] + words[step]) >>> 0;
        var big0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
        var majority = (a & b) ^ (a & c) ^ (b & c);
        var temp2 = (big0 + majority) >>> 0;
        h = g;
        g = f;
        f = e;
        e = (d + temp1) >>> 0;
        d = c;
        c = b;
        b = a;
        a = (temp1 + temp2) >>> 0;
      }
      hash[0] = (hash[0] + a) >>> 0;
      hash[1] = (hash[1] + b) >>> 0;
      hash[2] = (hash[2] + c) >>> 0;
      hash[3] = (hash[3] + d) >>> 0;
      hash[4] = (hash[4] + e) >>> 0;
      hash[5] = (hash[5] + f) >>> 0;
      hash[6] = (hash[6] + g) >>> 0;
      hash[7] = (hash[7] + h) >>> 0;
    }

    var result = '';
    for (var index = 0; index < 8; index++) {
      result += hash[index].toString(16).padStart(8, '0');
    }
    return result;
  }

  function hmacSha256Hex(message, key) {
    var keyBytes = toBytes(key);
    if (keyBytes.length > 64) keyBytes = hexToBytes(sha256Hex(key));
    while (keyBytes.length < 64) keyBytes.push(0);
    var inner = [], outer = [];
    for (var i = 0; i < 64; i++) {
      inner.push(keyBytes[i] ^ 0x36);
      outer.push(keyBytes[i] ^ 0x5c);
    }
    var innerHash = hexToBytes(sha256HexBytes(inner.concat(toBytes(message))));
    return sha256HexBytes(outer.concat(innerHash));
  }

  // ── crypto-js ──
  function makeWordArray(bytes) {
    return {
      _bytes: bytes,
      toString: function (encoder) {
        if (encoder && typeof encoder.stringify === 'function') return encoder.stringify(this);
        return bytesToHex(bytes);
      }
    };
  }

  function buildCryptoJs() {
    var Utf8 = {
      stringify: function (wordArray) { return utf8Decode(new Uint8Array(wordArray._bytes)); },
      parse: function (text) { return makeWordArray(toBytes(text)); }
    };
    var Hex = {
      stringify: function (wordArray) { return bytesToHex(wordArray._bytes); },
      parse: function (text) { return makeWordArray(hexToBytes(text)); }
    };
    var Base64 = {
      stringify: function (wordArray) { return base64Encode(new Uint8Array(wordArray._bytes)); },
      parse: function (text) { return makeWordArray(Array.prototype.slice.call(base64Decode(text))); }
    };
    function unsupported(name) {
      return function () {
        throw new Error('crypto-js.' + name + ' 暂未实现, 请把该插件的用法反馈给开发者');
      };
    }
    return {
      MD5: function (message) { return makeWordArray(hexToBytes(md5Hex(message))); },
      SHA256: function (message) { return makeWordArray(hexToBytes(sha256Hex(message))); },
      HmacSHA256: function (message, key) { return makeWordArray(hexToBytes(hmacSha256Hex(message, key))); },
      AES: { encrypt: unsupported('AES'), decrypt: unsupported('AES') },
      enc: { Utf8: Utf8, Latin1: Utf8, Hex: Hex, Base64: Base64 },
      lib: { WordArray: { create: function () { return makeWordArray([]); } } },
      mode: { CBC: 1, ECB: 2 },
      pad: { Pkcs7: 1 }
    };
  }

  // ── qs ──
  function buildQs() {
    function stringify(obj) {
      var parts = [];
      function add(key, value) {
        if (value === undefined || value === null) return;
        if (Array.isArray(value)) {
          for (var i = 0; i < value.length; i++) add(key + '[]', value[i]);
        } else if (typeof value === 'object') {
          for (var sub in value) add(key + '[' + sub + ']', value[sub]);
        } else {
          parts.push(encodeURIComponent(key) + '=' + encodeURIComponent(String(value)));
        }
      }
      for (var key in (obj || {})) add(key, obj[key]);
      return parts.join('&');
    }
    function parse(text) {
      var out = {};
      var source = String(text || '').replace(/^\?/, '');
      if (!source) return out;
      source.split('&').forEach(function (pair) {
        if (!pair) return;
        var index = pair.indexOf('=');
        var rawKey = index >= 0 ? pair.slice(0, index) : pair;
        var rawValue = index >= 0 ? pair.slice(index + 1) : '';
        var key, value;
        try { key = decodeURIComponent(rawKey.replace(/\+/g, ' ')); } catch (e) { key = rawKey; }
        try { value = decodeURIComponent(rawValue.replace(/\+/g, ' ')); } catch (e) { value = rawValue; }
        if (!(key in out)) out[key] = value;
        else if (Array.isArray(out[key])) out[key].push(value);
        else out[key] = [out[key], value];
      });
      return out;
    }
    return { stringify: stringify, parse: parse, encode: stringify, decode: parse };
  }

  // ── he ──
  var HTML_ENTITIES = {
    amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: '\u00a0',
    copy: '\u00a9', reg: '\u00ae', hellip: '\u2026', mdash: '\u2014', ndash: '\u2013',
    middot: '\u00b7', times: '\u00d7', divide: '\u00f7'
  };
  function heDecode(text) {
    return String(text).replace(/&(#x?[0-9a-fA-F]+|[a-zA-Z]+);/g, function (match, entity) {
      if (entity.charAt(0) === '#') {
        var code = entity.charAt(1).toLowerCase() === 'x'
          ? parseInt(entity.slice(2), 16)
          : parseInt(entity.slice(1), 10);
        return isNaN(code) ? match : String.fromCodePoint(code);
      }
      return Object.prototype.hasOwnProperty.call(HTML_ENTITIES, entity) ? HTML_ENTITIES[entity] : match;
    });
  }
  function heEncode(text) {
    return String(text).replace(/[&<>"']/g, function (ch) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch];
    });
  }

  // ── dayjs ──
  function buildDayjs() {
    function pad(value, length) {
      var text = String(value);
      while (text.length < length) text = '0' + text;
      return text;
    }
    function Dayjs(value) {
      if (value instanceof Dayjs) this.$d = new Date(value.$d.getTime());
      else if (value === undefined || value === null) this.$d = new Date();
      else this.$d = new Date(value);
    }
    Dayjs.prototype.valueOf = function () { return this.$d.getTime(); };
    Dayjs.prototype.unix = function () { return Math.floor(this.$d.getTime() / 1000); };
    Dayjs.prototype.toDate = function () { return this.$d; };
    Dayjs.prototype.isValid = function () { return !isNaN(this.$d.getTime()); };
    Dayjs.prototype.clone = function () { return new Dayjs(this.$d); };
    Dayjs.prototype.format = function (pattern) {
      var d = this.$d;
      var map = {
        YYYY: String(d.getFullYear()), YY: pad(d.getFullYear() % 100, 2),
        MM: pad(d.getMonth() + 1, 2), M: String(d.getMonth() + 1),
        DD: pad(d.getDate(), 2), D: String(d.getDate()),
        HH: pad(d.getHours(), 2), H: String(d.getHours()),
        mm: pad(d.getMinutes(), 2), m: String(d.getMinutes()),
        ss: pad(d.getSeconds(), 2), s: String(d.getSeconds()),
        SSS: pad(d.getMilliseconds(), 3)
      };
      return String(pattern || 'YYYY-MM-DDTHH:mm:ss').replace(/YYYY|YY|MM|DD|HH|mm|ss|SSS|M|D|H|m|s/g, function (token) {
        return map[token];
      });
    };
    Dayjs.prototype.add = function (amount, unit) {
      var d = new Date(this.$d.getTime());
      if (unit === 'day' || unit === 'd') d.setDate(d.getDate() + amount);
      else if (unit === 'month' || unit === 'M') d.setMonth(d.getMonth() + amount);
      else if (unit === 'year' || unit === 'y') d.setFullYear(d.getFullYear() + amount);
      else d.setSeconds(d.getSeconds() + amount);
      return new Dayjs(d);
    };
    Dayjs.prototype.subtract = function (amount, unit) { return this.add(-amount, unit); };
    function dayjs(value) { return new Dayjs(value); }
    dayjs.unix = function (seconds) { return new Dayjs(seconds * 1000); };
    dayjs.extend = function () { return dayjs; };
    dayjs.locale = function () { return dayjs; };
    return dayjs;
  }

  // ── big-integer ──
  function buildBigInteger() {
    function toBig(value) {
      if (value && typeof value === 'object' && typeof value.__big === 'bigint') return value.__big;
      if (typeof value === 'bigint') return value;
      var text = String(value).split('.')[0];
      if (!text) return BigInt(0);
      return BigInt(text);
    }
    function wrap(value) {
      return {
        __big: value,
        add: function (other) { return wrap(value + toBig(other)); },
        plus: function (other) { return wrap(value + toBig(other)); },
        subtract: function (other) { return wrap(value - toBig(other)); },
        minus: function (other) { return wrap(value - toBig(other)); },
        multiply: function (other) { return wrap(value * toBig(other)); },
        times: function (other) { return wrap(value * toBig(other)); },
        divide: function (other) { return wrap(value / toBig(other)); },
        mod: function (other) { return wrap(value % toBig(other)); },
        modPow: function (exp, mod) {
          var e = toBig(exp), m = toBig(mod);
          var result = BigInt(1), base = value % m, zero = BigInt(0), two = BigInt(2);
          while (e > zero) {
            if (e % two === BigInt(1)) result = (result * base) % m;
            base = (base * base) % m;
            e = e / two;
          }
          return wrap(result);
        },
        pow: function (exp) { return wrap(value ** toBig(exp)); },
        compare: function (other) {
          var target = toBig(other);
          return value < target ? -1 : value > target ? 1 : 0;
        },
        equals: function (other) { return value === toBig(other); },
        greater: function (other) { return value > toBig(other); },
        lesser: function (other) { return value < toBig(other); },
        isZero: function () { return value === BigInt(0); },
        isNegative: function () { return value < BigInt(0); },
        toString: function (radix) { return value.toString(radix || 10); },
        toJSNumber: function () { return Number(value); },
        valueOf: function () { return Number(value); }
      };
    }
    function bigInt(value) { return wrap(toBig(value === undefined ? 0 : value)); }
    bigInt.zero = bigInt(0);
    bigInt.one = bigInt(1);
    return bigInt;
  }

  // ── axios（基于同步 HTTP 桥）──
  function buildAxios(initialDefaults) {
    var defaults = initialDefaults || {
      headers: {},
      timeout: 0,
      validateStatus: function (status) { return status >= 200 && status < 300; }
    };
    if (!defaults.headers) defaults.headers = {};

    function mergeHeaders(target, source) {
      if (!source) return target;
      for (var key in source) {
        if (key === 'common' || key === 'get' || key === 'post' || key === 'put' ||
            key === 'patch' || key === 'delete' || key === 'head' || key === 'options') {
          var nested = source[key];
          if (nested && typeof nested === 'object') mergeHeaders(target, nested);
          continue;
        }
        if (key === 'toJSON' || key === 'set' || key === 'get' || key === 'has' ||
            key === 'delete' || key === 'clear') {
          if (typeof source[key] === 'function') continue;
        }
        var value = source[key];
        if (value === undefined || value === null) continue;
        if (typeof value === 'object' && typeof value.toString === 'function' && !Array.isArray(value)) {
          target[key] = String(value);
        } else {
          target[key] = String(value);
        }
      }
      return target;
    }

    function queryString(params) {
      if (!params) return '';
      if (typeof params === 'string') return params;
      var parts = [];
      for (var key in params) {
        var value = params[key];
        if (value === undefined || value === null) continue;
        if (Array.isArray(value)) {
          for (var i = 0; i < value.length; i++) {
            parts.push(encodeURIComponent(key) + '=' + encodeURIComponent(String(value[i])));
          }
        } else {
          parts.push(encodeURIComponent(key) + '=' + encodeURIComponent(String(value)));
        }
      }
      return parts.join('&');
    }

    function buildUrl(config) {
      var url = String(config.url || '');
      if (config.baseURL && url.indexOf('http') !== 0) {
        url = String(config.baseURL).replace(/\/+$/, '') + '/' + url.replace(/^\/+/, '');
      }
      var query = queryString(config.params);
      if (query) url += (url.indexOf('?') >= 0 ? '&' : '?') + query;
      return url;
    }

    function makeError(message, config, response) {
      var error = new Error(message);
      error.config = config;
      error.response = response;
      error.isAxiosError = true;
      error.code = response ? 'ERR_BAD_RESPONSE' : 'ERR_NETWORK';
      return error;
    }

    function doRequest(config) {
      config = config || {};
      var url = buildUrl(config);
      var method = String(config.method || 'GET').toUpperCase();
      var headers = mergeHeaders({}, defaults.headers);
      mergeHeaders(headers, config.headers);
      var body = config.data;
      if (body !== undefined && body !== null && typeof body !== 'string') {
        if (typeof body === 'object') {
          body = JSON.stringify(body);
          if (!headers['Content-Type'] && !headers['content-type']) {
            headers['Content-Type'] = 'application/json';
          }
        } else {
          body = String(body);
        }
      }
      var responseType = config.responseType || defaults.responseType || 'json';
      return new Promise(function (resolve, reject) {
        var raw = request({
          url: url,
          method: method,
          headers: headers,
          body: body === null || body === undefined ? null : String(body)
        });
        if (raw.error) {
          reject(makeError(raw.error, config, null));
          return;
        }
        var data = raw.body === undefined || raw.body === null ? '' : String(raw.body);
        if (responseType === 'arraybuffer') {
          data = utf8Encode(data).buffer;
        } else if (responseType !== 'text' && data) {
          try { data = JSON.parse(data); } catch (e) { /* 不是 JSON 就保留原文 */ }
        }
        var response = {
          data: data,
          status: raw.status,
          statusText: '',
          headers: lowerHeaders(raw.headers),
          config: config,
          // 浏览器版 axios 在响应上挂着 XHR 实例，插件常用 request.responseURL
          // 读取重定向后的最终地址；缺了它会让插件把成功的源判成失败。
          request: {
            responseURL: raw.url || url,
            status: raw.status,
            responseText: typeof data === 'string' ? data : ''
          }
        };
        var validate = config.validateStatus || defaults.validateStatus;
        if (validate(raw.status)) resolve(response);
        else reject(makeError('Request failed with status code ' + raw.status, config, response));
      });
    }

    function withMethod(method) {
      return function (url, config) {
        var merged = {};
        if (config) for (var key in config) merged[key] = config[key];
        merged.url = url;
        merged.method = method;
        return doRequest(merged);
      };
    }

    function withBody(method) {
      return function (url, data, config) {
        var merged = {};
        if (config) for (var key in config) merged[key] = config[key];
        merged.url = url;
        merged.method = method;
        merged.data = data;
        return doRequest(merged);
      };
    }

    var axios = function (config) { return doRequest(config); };
    axios.request = doRequest;
    axios.get = withMethod('GET');
    axios.delete = withMethod('DELETE');
    axios.head = withMethod('HEAD');
    axios.options = withMethod('OPTIONS');
    axios.post = withBody('POST');
    axios.put = withBody('PUT');
    axios.patch = withBody('PATCH');
    axios.defaults = defaults;
    axios.interceptors = {
      request: { use: function () { return 0; }, eject: noop, clear: noop },
      response: { use: function () { return 0; }, eject: noop, clear: noop }
    };
    axios.create = function (config) {
      var merged = {};
      for (var key in defaults) merged[key] = defaults[key];
      merged.headers = mergeHeaders({}, defaults.headers);
      if (config) {
        for (var option in config) {
          if (option === 'headers') mergeHeaders(merged.headers, config.headers);
          else merged[option] = config[option];
        }
      }
      return buildAxios(merged);
    };
    axios.all = function (list) { return Promise.all(list); };
    axios.spread = function (fn) {
      return function (values) { return fn.apply(null, values); };
    };
    axios.isAxiosError = function (error) { return !!(error && error.isAxiosError); };
    axios.default = axios;
    return axios;
  }

  // ── cheerio（jQuery 子集）──
  // DJ / 相声类插件用它解析站点 HTML。这里实现够用的 DOM 树 + CSS 选择器，
  // 覆盖 find/attr/text/html/each/map/eq 等常见调用。
  function buildCheerio() {
    var VOID_TAGS = {
      area: 1, base: 1, br: 1, col: 1, embed: 1, hr: 1, img: 1, input: 1,
      link: 1, meta: 1, param: 1, source: 1, track: 1, wbr: 1
    };

    function makeNode(type, name, attrs, parent) {
      return { type: type, name: name, attrs: attrs || {}, children: [], parent: parent || null, data: '' };
    }

    function appendText(parent, text) {
      if (!text) return;
      parent.children.push({ type: 'text', name: '', attrs: {}, children: [], parent: parent, data: text });
    }

    function findTagEnd(source, from) {
      var quote = '';
      for (var i = from; i < source.length; i++) {
        var ch = source.charAt(i);
        if (quote) {
          if (ch === quote) quote = '';
        } else if (ch === '"' || ch === "'") {
          quote = ch;
        } else if (ch === '>') {
          return i;
        }
      }
      return -1;
    }

    function parseAttrs(text) {
      var attrs = {};
      var re = /([^\s"'=<>\/]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+)))?/g;
      var match;
      while ((match = re.exec(text)) !== null) {
        var value = match[2] !== undefined ? match[2] : (match[3] !== undefined ? match[3] : (match[4] !== undefined ? match[4] : ''));
        attrs[match[1].toLowerCase()] = value;
      }
      return attrs;
    }

    function parseHtml(html) {
      var root = makeNode('root', '#root', {}, null);
      var stack = [root];
      var source = String(html === undefined || html === null ? '' : html);
      var cursor = 0;
      while (cursor < source.length) {
        var lt = source.indexOf('<', cursor);
        if (lt < 0) {
          appendText(stack[stack.length - 1], source.slice(cursor));
          break;
        }
        if (lt > cursor) appendText(stack[stack.length - 1], source.slice(cursor, lt));
        if (source.substr(lt, 4) === '<!--') {
          var commentEnd = source.indexOf('-->', lt + 4);
          cursor = commentEnd < 0 ? source.length : commentEnd + 3;
          continue;
        }
        if (source.charAt(lt + 1) === '!' || source.charAt(lt + 1) === '?') {
          var declEnd = source.indexOf('>', lt);
          cursor = declEnd < 0 ? source.length : declEnd + 1;
          continue;
        }
        var gt = findTagEnd(source, lt + 1);
        if (gt < 0) {
          appendText(stack[stack.length - 1], source.slice(lt));
          break;
        }
        var inside = source.slice(lt + 1, gt);
        var closing = inside.charAt(0) === '/';
        var selfClose = inside.charAt(inside.length - 1) === '/';
        var body = closing ? inside.slice(1) : inside;
        if (selfClose) body = body.slice(0, -1);
        var spaceAt = body.search(/\s/);
        var tagName = (spaceAt < 0 ? body : body.slice(0, spaceAt)).toLowerCase();
        var attrText = spaceAt < 0 ? '' : body.slice(spaceAt + 1);
        if (closing) {
          for (var depth = stack.length - 1; depth > 0; depth--) {
            if (stack[depth].name === tagName) {
              stack.length = depth;
              break;
            }
          }
        } else if (tagName) {
          var parent = stack[stack.length - 1];
          var node = makeNode('tag', tagName, parseAttrs(attrText), parent);
          parent.children.push(node);
          if (!VOID_TAGS[tagName] && !selfClose) stack.push(node);
        }
        cursor = gt + 1;
      }
      return root;
    }

    function textOf(node) {
      if (!node) return '';
      if (node.type === 'text') return node.data;
      var out = '';
      for (var i = 0; i < node.children.length; i++) out += textOf(node.children[i]);
      return out;
    }

    function innerHtml(node) {
      if (!node || !node.children) return '';
      var out = '';
      for (var i = 0; i < node.children.length; i++) out += serialize(node.children[i]);
      return out;
    }

    function serialize(node) {
      if (!node) return '';
      if (node.type === 'text') return node.data;
      if (node.type === 'root') return innerHtml(node);
      var out = '<' + node.name;
      for (var key in node.attrs) {
        out += ' ' + key + '="' + String(node.attrs[key]).replace(/"/g, '&quot;') + '"';
      }
      out += '>';
      if (VOID_TAGS[node.name]) return out;
      return out + innerHtml(node) + '</' + node.name + '>';
    }

    function parseCompound(token) {
      var tagMatch = token.match(/^([a-zA-Z][\w-]*|\*)/);
      var tag = tagMatch ? tagMatch[1].toLowerCase() : '*';
      var result = { tag: tag, id: '', classes: [], attrs: [] };
      var re = /#([\w-]+)|\.([\w-]+)|\[\s*([\w:.-]+)\s*(?:([~|^$*]?=)\s*(?:"([^"]*)"|'([^']*)'|([^\]\s]+)))?\s*\]/g;
      var match;
      while ((match = re.exec(token)) !== null) {
        if (match[1]) {
          result.id = match[1];
        } else if (match[2]) {
          result.classes.push(match[2]);
        } else if (match[3]) {
          var value = match[5] !== undefined ? match[5] : (match[6] !== undefined ? match[6] : (match[7] !== undefined ? match[7] : ''));
          result.attrs.push({ name: match[3].toLowerCase(), op: match[4] || '', value: value });
        }
      }
      return result;
    }

    function parseSelector(selector) {
      var groups = [];
      var parts = String(selector).split(',');
      for (var i = 0; i < parts.length; i++) {
        var trimmed = parts[i].trim();
        if (!trimmed) continue;
        var tokens = trimmed.split(/\s+/);
        var chain = [];
        for (var j = 0; j < tokens.length; j++) {
          if (tokens[j] === '>' || tokens[j] === '+' || tokens[j] === '~') continue;
          chain.push(parseCompound(tokens[j]));
        }
        if (chain.length) groups.push(chain);
      }
      return groups;
    }

    function matchesCompound(node, compound) {
      if (!node || node.type !== 'tag') return false;
      if (compound.tag !== '*' && node.name !== compound.tag) return false;
      if (compound.id && node.attrs.id !== compound.id) return false;
      if (compound.classes.length) {
        var classText = ' ' + (node.attrs['class'] || '') + ' ';
        for (var i = 0; i < compound.classes.length; i++) {
          if (classText.indexOf(' ' + compound.classes[i] + ' ') < 0) return false;
        }
      }
      for (var a = 0; a < compound.attrs.length; a++) {
        var spec = compound.attrs[a];
        var actual = node.attrs[spec.name];
        if (actual === undefined) return false;
        if (!spec.op) continue;
        actual = String(actual);
        if (spec.op === '=' && actual !== spec.value) return false;
        if (spec.op === '~=' && (' ' + actual + ' ').indexOf(' ' + spec.value + ' ') < 0) return false;
        if (spec.op === '^=' && actual.indexOf(spec.value) !== 0) return false;
        if (spec.op === '$=' && actual.slice(-spec.value.length) !== spec.value) return false;
        if (spec.op === '*=' && actual.indexOf(spec.value) < 0) return false;
        if (spec.op === '|=' && actual !== spec.value && actual.indexOf(spec.value + '-') !== 0) return false;
      }
      return true;
    }

    function matchChain(node, chain, index) {
      if (!matchesCompound(node, chain[index])) return false;
      if (index === 0) return true;
      var parent = node.parent;
      while (parent) {
        if (matchChain(parent, chain, index - 1)) return true;
        parent = parent.parent;
      }
      return false;
    }

    function matchesSelector(node, groups) {
      for (var i = 0; i < groups.length; i++) {
        if (matchChain(node, groups[i], groups[i].length - 1)) return true;
      }
      return false;
    }

    function collect(node, out) {
      var list = node.children || [];
      for (var i = 0; i < list.length; i++) {
        var child = list[i];
        if (child.type === 'tag') out.push(child);
        collect(child, out);
      }
      return out;
    }

    function Cheerio(nodes) {
      this._nodes = nodes || [];
      this.length = this._nodes.length;
    }

    Cheerio.prototype.each = function (fn) {
      for (var i = 0; i < this._nodes.length; i++) {
        if (fn.call(this._nodes[i], i, this._nodes[i]) === false) break;
      }
      return this;
    };
    Cheerio.prototype.map = function (fn) {
      var out = [];
      for (var i = 0; i < this._nodes.length; i++) {
        var value = fn.call(this._nodes[i], i, this._nodes[i]);
        if (value !== null && value !== undefined) out.push(value);
      }
      return new Cheerio(out);
    };
    Cheerio.prototype.get = function (index) {
      return index === undefined ? this._nodes.slice() : this._nodes[index];
    };
    Cheerio.prototype.toArray = function () { return this._nodes.slice(); };
    Cheerio.prototype.find = function (selector) {
      var groups = parseSelector(selector);
      var out = [];
      for (var i = 0; i < this._nodes.length; i++) {
        var candidates = collect(this._nodes[i], []);
        for (var j = 0; j < candidates.length; j++) {
          if (matchesSelector(candidates[j], groups) && out.indexOf(candidates[j]) < 0) {
            out.push(candidates[j]);
          }
        }
      }
      return new Cheerio(out);
    };
    Cheerio.prototype.filter = function (selector) {
      var groups = parseSelector(selector);
      var out = [];
      for (var i = 0; i < this._nodes.length; i++) {
        if (matchesSelector(this._nodes[i], groups)) out.push(this._nodes[i]);
      }
      return new Cheerio(out);
    };
    Cheerio.prototype.is = function (selector) {
      var groups = parseSelector(selector);
      for (var i = 0; i < this._nodes.length; i++) {
        if (matchesSelector(this._nodes[i], groups)) return true;
      }
      return false;
    };
    Cheerio.prototype.first = function () { return new Cheerio(this._nodes.slice(0, 1)); };
    Cheerio.prototype.last = function () { return new Cheerio(this._nodes.slice(-1)); };
    Cheerio.prototype.eq = function (index) {
      var real = index < 0 ? this._nodes.length + index : index;
      return new Cheerio(real >= 0 && real < this._nodes.length ? [this._nodes[real]] : []);
    };
    Cheerio.prototype.slice = function (start, end) { return new Cheerio(this._nodes.slice(start, end)); };
    Cheerio.prototype.text = function () {
      var out = '';
      for (var i = 0; i < this._nodes.length; i++) out += textOf(this._nodes[i]);
      return out;
    };
    Cheerio.prototype.html = function () {
      return this._nodes.length ? innerHtml(this._nodes[0]) : null;
    };
    Cheerio.prototype.attr = function (name) {
      if (this._nodes.length === 0) return undefined;
      return this._nodes[0].attrs[name];
    };
    Cheerio.prototype.val = function () { return this.attr('value'); };
    Cheerio.prototype.data = function (name) {
      if (!this._nodes.length) return undefined;
      return this._nodes[0].attrs['data-' + name];
    };
    Cheerio.prototype.children = function (selector) {
      var groups = selector ? parseSelector(selector) : null;
      var out = [];
      for (var i = 0; i < this._nodes.length; i++) {
        var list = this._nodes[i].children || [];
        for (var j = 0; j < list.length; j++) {
          if (list[j].type === 'tag' && (!groups || matchesSelector(list[j], groups))) out.push(list[j]);
        }
      }
      return new Cheerio(out);
    };
    Cheerio.prototype.parent = function () {
      var out = [];
      for (var i = 0; i < this._nodes.length; i++) {
        var parent = this._nodes[i].parent;
        if (parent && parent.type === 'tag' && out.indexOf(parent) < 0) out.push(parent);
      }
      return new Cheerio(out);
    };
    Cheerio.prototype.next = function () {
      var out = [];
      for (var i = 0; i < this._nodes.length; i++) {
        var parent = this._nodes[i].parent;
        if (!parent) continue;
        var index = parent.children.indexOf(this._nodes[i]);
        for (var j = index + 1; j < parent.children.length; j++) {
          if (parent.children[j].type === 'tag') { out.push(parent.children[j]); break; }
        }
      }
      return new Cheerio(out);
    };
    Cheerio.prototype.prev = function () {
      var out = [];
      for (var i = 0; i < this._nodes.length; i++) {
        var parent = this._nodes[i].parent;
        if (!parent) continue;
        var index = parent.children.indexOf(this._nodes[i]);
        for (var j = index - 1; j >= 0; j--) {
          if (parent.children[j].type === 'tag') { out.push(parent.children[j]); break; }
        }
      }
      return new Cheerio(out);
    };
    Cheerio.prototype.hasClass = function (name) {
      for (var i = 0; i < this._nodes.length; i++) {
        if ((' ' + (this._nodes[i].attrs['class'] || '') + ' ').indexOf(' ' + name + ' ') >= 0) return true;
      }
      return false;
    };

    function createQuery(root) {
      function $(target, context) {
        if (target === undefined || target === null) return new Cheerio([]);
        if (typeof target === 'string') {
          if (target.charAt(0) === '<') return new Cheerio(parseHtml(target).children);
          var groups = parseSelector(target);
          var scope;
          if (context) {
            scope = context instanceof Cheerio ? context._nodes : (context.type === 'tag' ? [context] : root.children);
          } else {
            scope = root.children;
          }
          var out = [];
          for (var i = 0; i < scope.length; i++) {
            if (matchesSelector(scope[i], groups) && out.indexOf(scope[i]) < 0) out.push(scope[i]);
            var candidates = collect(scope[i], []);
            for (var j = 0; j < candidates.length; j++) {
              if (matchesSelector(candidates[j], groups) && out.indexOf(candidates[j]) < 0) {
                out.push(candidates[j]);
              }
            }
          }
          return new Cheerio(out);
        }
        if (target instanceof Cheerio) return target;
        if (Array.isArray(target)) return new Cheerio(target);
        if (target.type === 'tag') return new Cheerio([target]);
        return new Cheerio([]);
      }
      $.load = function (html) { return createQuery(parseHtml(html)); };
      $.html = function (target) {
        if (target instanceof Cheerio) return innerHtml(target._nodes[0]);
        if (target) return serialize(target);
        return serialize(root);
      };
      $.root = function () { return new Cheerio([root]); };
      $.parseHTML = function (html) { return parseHtml(html).children; };
      $.text = function () { return textOf(root); };
      return $;
    }

    return {
      load: function (html) { return createQuery(parseHtml(html)); },
      html: function (html) { return createQuery(parseHtml(html)).html(); },
      parseHTML: function (html) { return parseHtml(html).children; }
    };
  }

  packages['axios'] = buildAxios(null);
  packages['cheerio'] = buildCheerio();
  packages['crypto-js'] = buildCryptoJs();
  packages['qs'] = buildQs();
  packages['he'] = { decode: heDecode, encode: heEncode, escape: heEncode, unescape: heDecode };
  packages['dayjs'] = buildDayjs();
  packages['big-integer'] = buildBigInteger();

  // Node 内置模块仍然不提供；MusicFree 注入的常用包已经就绪。
  g.require = function (name) {
    var pkg = packages[name];
    if (pkg) {
      if (!pkg.default) pkg.default = pkg;
      return pkg;
    }
    throw new Error('require is not available: ' + name);
  };

  g.crypto = {
    getRandomValues: function (arr) {
      for (var i = 0; i < arr.length; i++) arr[i] = Math.floor(Math.random() * 256);
      return arr;
    }
  };
})();
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::context::EvalOptions;
    use rquickjs::{Context, Runtime};

    /// 在 shim 环境里执行一段脚本，返回其字符串结果。
    fn eval(script: &str) -> String {
        let runtime = Runtime::new().expect("runtime");
        let context = Context::full(&runtime).expect("context");
        context.with(|ctx| {
            let mut options = EvalOptions::default();
            options.strict = false;
            let source = format!("{PRELUDE}\n{script}");
            ctx.eval_with_options::<String, _>(source.as_bytes(), options)
                .expect("eval")
        })
    }

    #[test]
    fn crypto_matches_known_vectors() {
        let result = eval(
            r#"
            var C = require('crypto-js');
            JSON.stringify({
              md5: C.MD5('abc').toString(),
              sha: C.SHA256('abc').toString(),
              hmac: C.HmacSHA256('abc', 'key').toString(),
              b64: C.enc.Base64.stringify(C.enc.Utf8.parse('中文abc'))
            });
            "#,
        );
        assert!(result.contains("900150983cd24fb0d6963f7d28e17f72"), "{result}");
        assert!(
            result.contains("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
            "{result}"
        );
        assert!(
            result.contains("9c196e32dc0175f86f4b1cb89289d6619de6bee699e4c378e68309ed97a1a6ab"),
            "{result}"
        );
        assert!(result.contains("5Lit5paHYWJj"), "{result}");
    }

    #[test]
    fn helper_packages_cover_common_usage() {
        let result = eval(
            r#"
            var qs = require('qs');
            var he = require('he');
            var dayjs = require('dayjs');
            var bigInt = require('big-integer');
            JSON.stringify({
              qs: qs.stringify({ a: 1, b: [2, 3] }),
              parsed: qs.parse('x=1&y=2'),
              entity: he.decode('a&amp;b&lt;c'),
              date: dayjs.unix(1700000000).format('YYYY-MM-DD HH:mm:ss'),
              big: bigInt('9007199254740993').add(1).toString()
            });
            "#,
        );
        assert!(result.contains("a=1&b%5B%5D=2&b%5B%5D=3"), "{result}");
        assert!(result.contains("\"y\":\"2\""), "{result}");
        assert!(result.contains("a&b<c"), "{result}");
        assert!(result.contains("2023-11-1"), "{result}");
        assert!(result.contains("9007199254740994"), "{result}");
    }

    #[test]
    fn cheerio_parses_and_queries_html() {
        let result = eval(
            r#"
            var cheerio = require('cheerio');
            var $ = cheerio.load('<div class="a"><a href="/x">标题</a><span>副</span></div><ul><li>1</li><li>2</li></ul>');
            var items = [];
            $('li').each(function (i, el) { items.push($(el).text()); });
            JSON.stringify({
              text: $('.a a').text(),
              href: $('a').attr('href'),
              count: $('div').length,
              items: items,
              inner: $('.a').html(),
              attrQuery: $('[href="/x"]').length
            });
            "#,
        );
        assert!(result.contains("\"text\":\"标题\""), "{result}");
        assert!(result.contains("\"href\":\"/x\""), "{result}");
        assert!(result.contains("\"count\":1"), "{result}");
        assert!(result.contains("\"items\":[\"1\",\"2\"]"), "{result}");
        assert!(result.contains("\"attrQuery\":1"), "{result}");
    }

    #[test]
    fn axios_exposes_the_common_surface() {
        // 只验证 API 形状；真实请求由集成测试覆盖。
        let result = eval(
            r#"
            var axios = require('axios');
            var instance = axios.create({ baseURL: 'https://example.com' });
            JSON.stringify({
              hasGet: typeof axios.get === 'function',
              hasPost: typeof axios.post === 'function',
              hasRequest: typeof instance.request === 'function',
              hasInterceptors: typeof axios.interceptors.response.use === 'function',
              baseURL: instance.getUri ? instance.getUri() : instance.defaults.baseURL
            });
            "#,
        );
        assert!(result.contains("\"hasGet\":true"), "{result}");
        assert!(result.contains("\"hasPost\":true"), "{result}");
        assert!(result.contains("\"hasRequest\":true"), "{result}");
        assert!(result.contains("\"hasInterceptors\":true"), "{result}");
    }
}
