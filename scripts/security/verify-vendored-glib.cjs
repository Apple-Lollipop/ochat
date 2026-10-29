/* Verify the audited source package and the exact upstream security backport. */
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),directory=path.join(root,'vendor/glib-0.18.5');
const expected=JSON.parse(fs.readFileSync(path.join(root,'vendor/glib-0.18.5-upstream-sha256.json'),'utf8')).files;
expected['src/variant_iter.rs']='a0f5ee8acb8faa089bcdfbc9a57372609fce7654026ccef7d9a224d05a654ccc';
const actual=new Map();
function walk(folder){for(const entry of fs.readdirSync(folder,{withFileTypes:true})){assert.ok(!entry.isSymbolicLink(),'Vendored symlinks are not permitted');const file=path.join(folder,entry.name);if(entry.isDirectory())walk(file);else actual.set(path.relative(directory,file).split(path.sep).join('/'),file);}}
walk(directory);
assert.deepEqual([...actual.keys()].sort(),Object.keys(expected).sort(),'Unexpected added or missing vendored files');
for(const [name,digest]of Object.entries(expected))assert.equal(crypto.createHash('sha256').update(fs.readFileSync(actual.get(name))).digest('hex'),digest,'Vendor integrity mismatch: '+name);
console.log('GLib source integrity passed: '+actual.size+' files, only the documented upstream fix');
