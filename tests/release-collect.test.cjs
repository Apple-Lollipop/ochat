/* Isolated packaging fixtures; these small files are not real executables. */
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
function fixture(t){
 const temp=fs.realpathSync(os.tmpdir()),root=fs.mkdtempSync(path.join(temp,'ochat-release-test-'));
 t.after(()=>{const resolved=fs.realpathSync(root),relative=path.relative(temp,resolved);assert.ok(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative)&&path.basename(resolved).startsWith('ochat-release-test-'));fs.rmSync(resolved,{recursive:true,force:true});});
 const write=(name,bytes)=>{const file=path.join(root,name);fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,bytes);return file;};
 write('package.json',JSON.stringify({version:'0.0.1'}));write('scripts/release/collect.cjs',fs.readFileSync(path.join(__dirname,'../scripts/release/collect.cjs')));
 const run=(platform,directory)=>spawnSync(process.execPath,[path.join(root,'scripts/release/collect.cjs'),platform,...(directory?[path.join(root,directory)]:[])],{cwd:root,encoding:'utf8'});
 const manifest=platform=>JSON.parse(fs.readFileSync(path.join(root,'releases',platform,'release.json'),'utf8'));
 return {root,write,run,manifest};
}
function success(result){assert.equal(result.status,0,result.stderr);}
test('Linux collection finds fresh Tauri bundles in a clean CI workspace and verifies their hashes',t=>{
 const f=fixture(t);f.write('target/release/bundle/deb/Ochat_0.0.1_amd64.deb','fresh deb');f.write('target/release/bundle/appimage/Ochat_0.0.1_amd64.AppImage','fresh image');f.write('target/release/bundle/deb/Ochat_0.0.10_amd64.deb','other version');
 success(f.run('linux'));const manifest=f.manifest('Linux');assert.equal(manifest.files.length,2);
 for(const entry of manifest.files){const bytes=fs.readFileSync(path.join(f.root,'releases/Linux',entry.name));assert.equal(entry.bytes,bytes.length);assert.equal(entry.sha256,crypto.createHash('sha256').update(bytes).digest('hex'));}
 assert.equal(fs.readFileSync(path.join(f.root,'releases/Linux/Ochat_0.0.1_amd64.deb'),'utf8'),'fresh deb');assert.equal(fs.existsSync(path.join(f.root,'releases/Linux/Ochat_0.0.10_amd64.deb')),false);
 assert.equal(fs.readFileSync(path.join(f.root,'releases/Linux/SHA256SUMS'),'utf8').trim().split('\n').length,2);
});
test('Linux collection retains the local already-staged workflow',t=>{
 const f=fixture(t);f.write('releases/Linux/Ochat_0.0.1_amd64.deb','staged deb');f.write('releases/Linux/Ochat_0.0.1_amd64.AppImage','staged image');success(f.run('linux'));assert.equal(f.manifest('Linux').files.length,2);
});
test('a native Linux build takes precedence without advertising stale staged bundles',t=>{
 const f=fixture(t);f.write('target/release/bundle/deb/Ochat_0.0.1_amd64.deb','new deb');f.write('releases/Linux/Ochat_0.0.1_amd64.deb','old deb');f.write('releases/Linux/Ochat_0.0.1_amd64.AppImage','old image');
 success(f.run('linux'));assert.deepEqual(f.manifest('Linux').files.map(entry=>entry.name),['Ochat_0.0.1_amd64.deb']);assert.equal(fs.readFileSync(path.join(f.root,'releases/Linux/Ochat_0.0.1_amd64.deb'),'utf8'),'new deb');
});
test('collection rejects a different version instead of claiming it is the requested release',t=>{
 const f=fixture(t);f.write('target/release/bundle/deb/Ochat_0.0.10_amd64.deb','other release');const result=f.run('linux');assert.notEqual(result.status,0);assert.match(result.stderr,/No real linux installer found for 0\.0\.1/);assert.equal(fs.existsSync(path.join(f.root,'releases/Linux/release.json')),false);
});
test('Windows collection preserves the NSIS installer and standalone executable',t=>{
 const f=fixture(t);f.write('target/release/bundle/nsis/Ochat_0.0.1_x64-setup.exe','installer');f.write('target/release/ochat.exe','executable');success(f.run('windows'));assert.deepEqual(f.manifest('Windows').files.map(entry=>entry.name).sort(),['Ochat_0.0.1_x64-setup.exe','Ochat_0.0.1_x64.exe']);
});
test('macOS collection defaults to Apple Silicon and excludes older universal output',t=>{
 const f=fixture(t);f.write('target/aarch64-apple-darwin/release/bundle/dmg/Ochat_0.0.1_aarch64.dmg','arm64');f.write('target/universal-apple-darwin/release/bundle/dmg/Ochat_0.0.1_universal.dmg','older build');success(f.run('macos'));assert.deepEqual(f.manifest('macOS').files.map(entry=>entry.name),['Ochat_0.0.1_aarch64.dmg']);
});
test('explicit collection directories retain the dedicated arm64 macOS workflow',t=>{
 const f=fixture(t),directory='target/aarch64-apple-darwin/release/bundle/dmg';f.write(directory+'/Ochat_0.0.1_aarch64.dmg','arm64');f.write('target/universal-apple-darwin/release/bundle/dmg/Ochat_0.0.1_universal.dmg','other build');success(f.run('macos',directory));assert.deepEqual(f.manifest('macOS').files.map(entry=>entry.name),['Ochat_0.0.1_aarch64.dmg']);
});
