// Bounded metadata-only probes, outside sfw, for a connectivity control.
// Keep TLS validation/SNI enabled; never log headers, bodies or environment.
const dns = require('node:dns').promises;
const https = require('node:https');
const net = require('node:net');

async function probe(host, address, autoSelectFamily) {
  const started = Date.now();
  return new Promise((resolve) => {
    let remote;
    const request = https.get({
      hostname: host,
      path: host === 'index.crates.io' ? '/config.json' : '/',
      agent: false,
      autoSelectFamily,
      ...(address ? { lookup: (_host, options, callback) => {
        if (options.all) callback(null, [{ address, family: 4 }]);
        else callback(null, address, 4);
      } } : {}),
    }, (response) => {
      response.resume();
      finish({ status: response.statusCode });
    });
    const timer = setTimeout(() => request.destroy(Object.assign(new Error('probe timeout'), { code: 'PROBE_TIMEOUT' })), 5000);
    let done = false;
    function finish(result) {
      if (done) return;
      done = true;
      clearTimeout(timer);
      console.log(JSON.stringify({ host, address: address || 'DNS', autoSelectFamily,
        remote, elapsedMs: Date.now() - started, ...result }));
      resolve();
    }
    request.on('socket', (socket) => socket.once('connect', () => { remote = socket.remoteAddress; }));
    request.on('error', (error) => finish({ error: error.code }));
  });
}

(async () => {
  console.log(JSON.stringify({ node: process.version, platform: process.platform,
    autoSelectFamilyDefault: net.getDefaultAutoSelectFamily() }));
  for (const host of ['index.crates.io', 'static.crates.io']) {
    console.log(JSON.stringify({ host, lookup: await dns.lookup(host, { all: true }) }));
    for (let attempt = 1; attempt <= 2; attempt++) {
      console.log(JSON.stringify({ host, attempt }));
      await Promise.all(['151.101.2.137', '151.101.66.137', '151.101.130.137', '151.101.194.137']
        .map((address) => probe(host, address, false)));
      await probe(host, null, false);
      await probe(host, null, true);
    }
  }
})().catch((error) => { console.error(error.code || error.name); process.exitCode = 1; });
