const fs=require('node:fs'),path=require('node:path'),{spawnSync}=require('node:child_process');
const {VERSION}=require('./check-version.cjs');
function verifyBuildRun(run,jobs,{repository,commit}){
 if(run.repository?.full_name!==repository||run.head_sha!==commit||run.status!=='completed'||!['push','workflow_dispatch'].includes(run.event)||run.path!=='.github/workflows/build.yml')throw Error('The source run does not match this repository, commit and desktop workflow');
 const required=['windows-latest','ubuntu-24.04','Apple Silicon'],builders=[];
 for(const label of required){const matches=jobs.filter(job=>job.name.includes(label));if(matches.length!==1||matches[0].conclusion!=='success')throw Error('The original '+label+' build did not succeed');builders.push(matches[0]);}
 if(jobs.some(job=>!builders.includes(job)&&job.name!=='Publish GitHub release'))throw Error('Unexpected jobs in the source workflow');
 return true;
}
module.exports={verifyBuildRun};
if(require.main===module){try{
 const repository=process.env.GITHUB_REPOSITORY,runId=process.env.SOURCE_RUN,tag=process.env.RELEASE_TAG,source=path.resolve(process.env.RELEASE_SOURCE_DIR);
 if(!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)||!/^[1-9]\d*$/.test(runId))throw Error('Invalid source workflow context');
 const version=JSON.parse(fs.readFileSync(path.join(source,'package.json'),'utf8')).version;if(!VERSION.test(version)||tag!=='v'+version)throw Error('Recovery tag must match the checked-out source version');
 function command(binary,args,cwd){const result=spawnSync(binary,args,{cwd,encoding:'utf8',env:{...process.env,GH_HOST:'github.com',GH_REPO:repository}});if(result.status!==0)throw Error(binary+' could not verify the source workflow');return result.stdout;}
 const commit=command('git',['rev-parse','HEAD'],source).trim(),prefix='repos/'+repository+'/actions/runs/'+runId;
 const run=JSON.parse(command('gh',['api',prefix])),pages=JSON.parse(command('gh',['api',prefix+'/jobs?per_page=100','--paginate','--slurp']));
 verifyBuildRun(run,pages.flatMap(page=>page.jobs),{repository,commit});console.log('Original three-platform builds verified for '+tag+' at '+commit);
}catch(error){console.error(error.message);process.exitCode=1;}}
