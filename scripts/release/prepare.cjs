const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {VERSION}=require('./check-version.cjs');
function expectedFiles(version){return {
 Windows:['Ochat_'+version+'_x64-setup.exe','Ochat_'+version+'_x64.exe'],
 Linux:['Ochat_'+version+'_amd64.deb','Ochat_'+version+'_amd64.AppImage'],
 macOS:['Ochat_'+version+'_aarch64.dmg']
};}
function hash(file){return crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');}
function regular(file){if(!fs.lstatSync(file).isFile())throw Error('Expected a regular file: '+path.basename(file));}
function prepare(input,output,tag,version){
 if(!VERSION.test(version)||tag!=='v'+version)throw Error('Release tag must match the project version');
 input=path.resolve(input);output=path.resolve(output);
 const relative=path.relative(input,output);
 if(output===input||!(relative==='..'||relative.startsWith('..'+path.sep)||path.isAbsolute(relative)))throw Error('Release output must be separate from downloaded artifacts');
 const expected=expectedFiles(version),verified=[];
 for(const [folder,names] of Object.entries(expected)){
  const directory=path.join(input,folder),manifestFile=path.join(directory,'release.json');regular(manifestFile);
  const manifest=JSON.parse(fs.readFileSync(manifestFile,'utf8')),platform=folder==='macOS'?'macos':folder.toLowerCase();
  if(manifest.version!==version||manifest.platform!==platform||manifest.status!=='built')throw Error('Invalid '+folder+' release manifest');
  if(!Array.isArray(manifest.files)||manifest.files.length!==names.length||new Set(manifest.files.map(f=>f.name)).size!==names.length||names.some(name=>!manifest.files.some(f=>f.name===name)))throw Error('Incomplete or unexpected '+folder+' release files');
  for(const entry of manifest.files){
   const source=path.join(directory,entry.name);regular(source);
   if(!Number.isSafeInteger(entry.bytes)||entry.bytes<=0||fs.statSync(source).size!==entry.bytes||!/^[a-f0-9]{64}$/.test(entry.sha256)||hash(source)!==entry.sha256)throw Error('Artifact integrity check failed: '+entry.name);
   verified.push({name:entry.name,bytes:entry.bytes,sha256:entry.sha256,source});
  }
  if(folder==='macOS'){
   const validationFile=path.join(directory,'validation.json');regular(validationFile);
   if(JSON.parse(fs.readFileSync(validationFile,'utf8')).verification!=='success')throw Error('The macOS signature and architecture verification did not succeed');
  }
 }
 fs.mkdirSync(output,{recursive:true});
 const files=[];
 for(const entry of verified){
  const target=path.join(output,entry.name);if(fs.existsSync(target))regular(target);fs.copyFileSync(entry.source,target);
  if(hash(target)!==entry.sha256)throw Error('Release copy changed: '+entry.name);
  files.push({name:entry.name,bytes:entry.bytes,sha256:entry.sha256});
 }
 const checksum=path.join(output,'SHA256SUMS.txt');if(fs.existsSync(checksum))regular(checksum);
 fs.writeFileSync(checksum,files.map(f=>f.sha256+'  '+f.name).join('\n')+'\n');
 files.push({name:'SHA256SUMS.txt',bytes:fs.statSync(checksum).size,sha256:hash(checksum)});
 const plan={version,tag,files};
 const planFile=path.join(output,'release-plan.json');if(fs.existsSync(planFile))regular(planFile);
 fs.writeFileSync(planFile,JSON.stringify(plan,null,2)+'\n');
 return plan;
}
module.exports={prepare,expectedFiles,hash,regular};
if(require.main===module){try{const source=path.resolve(process.env.RELEASE_SOURCE_DIR||path.join(__dirname,'../..'));const version=JSON.parse(fs.readFileSync(path.join(source,'package.json'),'utf8')).version;const plan=prepare(process.argv[2],process.argv[3],process.argv[4],version);console.log('Verified '+plan.files.length+' release assets for '+plan.tag);}catch(error){console.error(error.message);process.exitCode=1;}}
