// Cache availability must never decide whether project checks run.
const fs=require('node:fs'),{spawnSync}=require('node:child_process');
function configure({env=process.env,run=spawnSync,append=text=>fs.appendFileSync(env.GITHUB_ENV,text),log=console.log}={}){
 const probe=run('sccache',['rustc','-vV'],{env:{...env,SCCACHE_IGNORE_SERVER_IO_ERROR:'0'},timeout:20000,stdio:'ignore'});
 if(probe.status===0){log('Compiler cache available.');return true;}
 append('RUSTC_WRAPPER=\nSCCACHE_GHA_ENABLED=false\n');
 // Cache errors can contain signed storage URLs; do not echo raw stderr.
 log('::warning::Compiler cache unavailable; all project checks will use rustc directly.');
 return false;
}
module.exports={configure};
if(require.main===module)configure();
