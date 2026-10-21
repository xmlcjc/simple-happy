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

  // 定时器只用于把回调推进微任务队列, execute_pending_job 会统一驱动。
  g.setTimeout = function (fn) {
    if (typeof fn === 'function') Promise.resolve().then(function () { try { fn(); } catch (e) {} });
    return 0;
  };
  g.clearTimeout = noop;
  g.setInterval = function () { return 0; };
  g.clearInterval = noop;

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
    this.responseText = res.body || '';
    this.response = this.responseText;
    if (typeof this.onreadystatechange === 'function') this.onreadystatechange();
    if (typeof this.onload === 'function') this.onload();
    if (typeof this.onloadend === 'function') this.onloadend();
  };
  g.XMLHttpRequest.prototype.abort = noop;

  // Node 内置模块不提供, 明确报错而不是让插件静默拿到 undefined。
  g.require = function (name) {
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
