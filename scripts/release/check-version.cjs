const fs=require('node:fs'),path=require('node:path'),{spawnSync}=require('node:child_process');
const VERSION=/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/;
function validateVersions(versions,tag=''){
 const version=versions.npm;
 if(typeof version!=='string'||!VERSION.test(version))throw Error('Invalid project release version');
 if(tag&&tag!=='v'+version)throw Error('Release tag must be v'+version+'; received '+tag);
 for(const key of ['lock','lockRoot','tauri','ochat','ochat-core'])if(versions[key]!==version)throw Error('Version mismatch in '+key+': expected '+version);
 return version;
}
function validateCargoLock(text,version){
 const packages=text.split(/^\[\[package\]\]\s*$/m).slice(1);
 for(const name of ['ochat','ochat-core']){
  const entries=packages.filter(block=>block.match(/^name\s*=\s*"([^"]+)"\s*$/m)?.[1]===name&&!/^source\s*=/m.test(block));
  if(entries.length!==1||entries[0].match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1]!==version)throw Error('Version mismatch in Cargo.lock for '+name+': expected '+version);
 }
}
function checkProjectVersion(root,tag=''){
 const read=name=>JSON.parse(fs.readFileSync(path.join(root,name),'utf8'));
 const npm=read('package.json'),lock=read('package-lock.json'),tauri=read('src-tauri/tauri.conf.json');
 if(tag&&tag!=='v'+npm.version)throw Error('Release tag must be v'+npm.version+'; received '+tag);
 const result=spawnSync('cargo',['metadata','--no-deps','--format-version','1','--locked'],{cwd:root,encoding:'utf8'});
 if(result.status!==0)throw Error('Could not validate Cargo versions: '+(result.stderr||result.error?.message||'cargo metadata failed'));
 const metadata=JSON.parse(result.stdout),versions={npm:npm.version,lock:lock.version,lockRoot:lock.packages?.['']?.version,tauri:tauri.version};
 for(const name of ['ochat','ochat-core'])versions[name]=metadata.packages.find(p=>p.name===name)?.version;
 const version=validateVersions(versions,tag);
 validateCargoLock(fs.readFileSync(path.join(root,'Cargo.lock'),'utf8'),version);
 return version;
}
module.exports={validateVersions,validateCargoLock,checkProjectVersion,VERSION};
if(require.main===module){try{const version=checkProjectVersion(path.resolve(__dirname,'../..'),process.env.RELEASE_TAG||'');console.log('Project versions verified: '+version);}catch(error){console.error(error.message);process.exitCode=1;}}
