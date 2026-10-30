const {test}=require('node:test'),assert=require('node:assert/strict');
const {configure}=require('../scripts/ci/compiler-cache.cjs');
test('a healthy cache keeps normal compiler wrapping without modifying workflow state',()=>{
 const writes=[],logs=[];
 assert.equal(configure({env:{SCCACHE_IGNORE_SERVER_IO_ERROR:'1'},run:(binary,args,options)=>{assert.equal(binary,'sccache');assert.deepEqual(args,['rustc','-vV']);assert.equal(options.env.SCCACHE_IGNORE_SERVER_IO_ERROR,'0');assert.equal(options.timeout,20000);return {status:0};},append:text=>writes.push(text),log:text=>logs.push(text)}),true);
 assert.deepEqual(writes,[]);
});
test('cache outage and missing installation disable only the cache and never echo storage credentials',()=>{
 for(const result of [{status:2,stderr:'ServerBusy signed-private-url'},{status:null,error:Error('ENOENT')},{status:null,error:Error('ETIMEDOUT')}]){
  let written='',output='';assert.equal(configure({run:()=>result,append:text=>written+=text,log:text=>output+=text}),false);
  assert.equal(written,'RUSTC_WRAPPER=\nSCCACHE_GHA_ENABLED=false\n');assert.ok(!output.includes('signed-private-url'));
 }
});
