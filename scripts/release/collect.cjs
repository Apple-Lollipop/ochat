// Stage only the current version's real installers; never fabricate missing ones.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../..'),version=require(path.join(root,'package.json')).version;
const platform=process.argv[2],names={windows:'Windows',linux:'Linux',macos:'macOS'};
if(!names[platform])throw Error('Use windows, linux or macos');
const destination=path.join(root,'releases',names[platform]);
const extension=platform==='windows'?/setup\.exe$/i:platform==='linux'?/\.(deb|AppImage)$/:/\.dmg$/i;
const find=locations=>locations.flatMap(directory=>fs.existsSync(directory)?fs.readdirSync(directory,{withFileTypes:true}).filter(entry=>entry.isFile()&&entry.name.includes(`_${version}_`)&&extension.test(entry.name)).map(entry=>path.join(directory,entry.name)):[]);
const nativeLinux=[path.join(root,'target/release/bundle/deb'),path.join(root,'target/release/bundle/appimage')];
const locations=process.argv[3]?[path.resolve(process.argv[3])]:platform==='windows'?[path.join(root,'target/release/bundle/nsis')]:platform==='linux'?nativeLinux:[path.join(root,'target/aarch64-apple-darwin/release/bundle/dmg')];
let installers=find(locations);
// CI invokes Tauri directly; local Linux builds may already have staged their files.
// Prefer one native build over mixing its output with older staged files.
if(platform==='linux'&&!process.argv[3]&&!installers.length)installers=find([destination]);
if(!installers.length)throw Error('No real '+platform+' installer found for '+version);
fs.mkdirSync(destination,{recursive:true});
const files=installers.map(source=>{const name=path.basename(source),file=path.join(destination,name);if(path.resolve(source)!==file)fs.copyFileSync(source,file);const bytes=fs.readFileSync(file);return {name,bytes:bytes.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex')};});
if(platform==='windows'){
  const source=path.join(root,'target/release/ochat.exe');
  if(!fs.existsSync(source))throw Error('The Windows application executable is missing');
  const name=`Ochat_${version}_x64.exe`,file=path.join(destination,name);
  fs.copyFileSync(source,file);const bytes=fs.readFileSync(file);
  files.push({name,bytes:bytes.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex')});
}
fs.writeFileSync(path.join(destination,'SHA256SUMS'),files.map(file=>file.sha256+'  '+file.name).join('\n')+'\n');
fs.writeFileSync(path.join(destination,'release.json'),JSON.stringify({version,platform,status:'built',files},null,2)+'\n');
console.log(JSON.stringify({version,platform,files},null,2));
