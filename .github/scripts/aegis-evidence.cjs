// Temporary CI evidence collector. Print only package policy fields.
const fs = require('node:fs');
const cp = require('node:child_process');
const path = require('node:path');
const file = process.platform === 'win32'
  ? path.join(process.env.ProgramData || 'C:\\ProgramData', 'Aegis', 'service.jsonl')
  : process.platform === 'darwin'
    ? '/Library/Application Support/Aegis/service.jsonl'
    : '/var/log/aegis/service.jsonl';
let data;
try { data = fs.readFileSync(file,'utf8'); }
catch {
  if (process.platform === 'win32') throw Error('Cannot read Aegis audit log');
  data = cp.execFileSync('sudo', ['cat', file], {encoding:'utf8'});
}
let decisions = 0;
for (const line of data.split('\n')) {
  let row;
  try { row = JSON.parse(line); } catch { continue; }
  if (row.msg !== 'package decision') continue;
  decisions++;
  if (row.action === 'block' || /(?:iddqd|cargo-llvm-cov|cargo-nextest|nanoid)@/.test(row.purl || '')) {
    console.log('AEGIS_EVIDENCE '+JSON.stringify(Object.fromEntries(
      ['time','purl','action','reason','request_id'].filter(k=>k in row).map(k=>[k,row[k]])
    )));
  }
}
console.log('AEGIS_PACKAGE_DECISIONS '+decisions);
