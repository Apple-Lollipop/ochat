const fs=require('node:fs'),path=require('node:path'),{spawnSync}=require('node:child_process');
const {expectedFiles,hash,regular}=require('./prepare.cjs'),{VERSION}=require('./check-version.cjs');
function publish(planFile,{tag,version,repository,commit,run}){
 if(!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)||!/^[a-f0-9]{40}$/.test(commit))throw Error('Invalid GitHub release context');
 regular(planFile);const plan=JSON.parse(fs.readFileSync(planFile,'utf8')),directory=path.dirname(path.resolve(planFile));
 if(!VERSION.test(version)||tag!=='v'+version||plan.tag!==tag||plan.version!==version)throw Error('Release plan does not match the tag and project version');
 const names=[...Object.values(expectedFiles(version)).flat(),'SHA256SUMS.txt'];
 if(!Array.isArray(plan.files)||plan.files.length!==names.length||new Set(plan.files.map(f=>f.name)).size!==names.length||plan.files.some(f=>!names.includes(f.name)))throw Error('Release plan has unexpected assets');
 for(const file of plan.files){const asset=path.join(directory,file.name);regular(asset);if(fs.statSync(asset).size!==file.bytes||hash(asset)!==file.sha256)throw Error('Release asset changed: '+file.name);}
 const runner=run||((args)=>spawnSync('gh',args,{encoding:'utf8',env:{...process.env,GH_HOST:'github.com',GH_REPO:repository}}));
 function gh(args,missing=false){
  const result=runner(args);if(result.status===0)return result.stdout||'';
  if(missing&&/HTTP 404/.test(result.stderr||''))return null;
  const detail=(result.stderr||result.error?.message||'GitHub CLI failed').slice(0,2000),token=process.env.GH_TOKEN;
  throw Error(token?detail.split(token).join('[redacted]'):detail);
 }
 const endpoint='repos/'+repository+'/releases/tags/'+encodeURIComponent(tag);
 function release(){const result=gh(['api',endpoint],true);return result===null?null:JSON.parse(result);}
 function verifyTag(){
  let object=JSON.parse(gh(['api','repos/'+repository+'/git/ref/tags/'+encodeURIComponent(tag)])).object;
  for(let n=0;object?.type==='tag'&&n<8;n++)object=JSON.parse(gh(['api','repos/'+repository+'/git/tags/'+object.sha])).object;
  if(object?.type!=='commit'||object.sha!==commit)throw Error('The remote tag no longer points to the built commit');
 }
 function verifyAssets(value){
  if(!Array.isArray(value.assets)||value.assets.length!==plan.files.length)throw Error('Remote release assets are incomplete or unexpected');
  for(const file of plan.files){const asset=value.assets.find(a=>a.name===file.name);if(!asset||asset.state!=='uploaded'||asset.size!==file.bytes||asset.digest!=='sha256:'+file.sha256)throw Error('GitHub asset verification failed: '+file.name);}
 }
 verifyTag();let current=release();
 if(current&&!current.draft){verifyAssets(current);return {url:current.html_url,alreadyPublished:true};}
 if(current&&current.target_commitish!==commit)throw Error('Existing draft belongs to another source commit');
 if(!current){
  gh(['release','create',tag,'--draft','--verify-tag','--target',commit,'--title','Ochat '+version,'--generate-notes',...(version.includes('-')?['--prerelease']:[])]);
  current=release();if(!current?.draft||current.target_commitish!==commit)throw Error('Could not create the expected draft release');
 }
 const existing=current.assets||[];
 for(const asset of existing){const file=plan.files.find(f=>f.name===asset.name);if(!file||asset.state!=='uploaded'||asset.size!==file.bytes||asset.digest!=='sha256:'+file.sha256)throw Error('Existing draft has a different or incomplete asset: '+asset.name);}
 const missing=plan.files.filter(file=>!existing.some(asset=>asset.name===file.name));
 if(missing.length)gh(['release','upload',tag,...missing.map(f=>path.join(directory,f.name))]);
 current=release();verifyAssets(current);verifyTag();
 gh(['release','edit',tag,'--draft=false']);
 current=release();if(!current||current.draft)throw Error('Release is still a draft');verifyAssets(current);
 return {url:current.html_url,alreadyPublished:false};
}
module.exports={publish};
if(require.main===module){try{
 if(!process.env.GH_TOKEN)throw Error('GH_TOKEN is required');
 const result=spawnSync('git',['rev-parse','HEAD'],{encoding:'utf8'});if(result.status!==0)throw Error('Could not identify the checked-out commit');
 const published=publish(process.argv[2],{tag:process.env.RELEASE_TAG,version:require('../../package.json').version,repository:process.env.GITHUB_REPOSITORY,commit:result.stdout.trim()});
 console.log('Release available: '+published.url);
}catch(error){console.error(error.message);process.exitCode=1;}}
