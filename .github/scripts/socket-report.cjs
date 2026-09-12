// Temporary validation evidence: never print credentials or the full environment.
const fs = require('node:fs');
const reportPath = process.env.SFW_JSON_REPORT_PATH;
if (!reportPath || !fs.existsSync(reportPath)) {
  console.log('::warning::No Socket report was produced; no cooldown evidence available');
  process.exit(0);
}
const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
const fields = [];
const decisions = [];
const allowed = new Set(['purl', 'name', 'version', 'type', 'action', 'severity', 'alertType', 'title']);
function visit(value, path = []) {
  if (value && typeof value === 'object') {
    for (const [key, child] of Object.entries(value)) visit(child, [...path, key]);
  } else {
    fields.push(path.join('.'));
    if (allowed.has(path.at(-1)) || value === 'recentlyPublished') {
      decisions.push({ field: path.join('.'), value });
    }
  }
}
visit(report);
console.log(JSON.stringify({ fields, decisions }, null, 2));
