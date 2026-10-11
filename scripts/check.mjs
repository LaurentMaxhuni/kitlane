import { readFile, access, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
const root=path.resolve(fileURLToPath(new URL('..',import.meta.url)));
const failures=[];let checks=0;
function check(condition,message){checks++;if(!condition)failures.push(message);}
const tokens=JSON.parse(await readFile(path.join(root,'tokens/tokens.json'),'utf8'));
const resolve=v=>v.startsWith('{')?tokens.color[v.slice(1,-1).split('.')[1]].$value:v;
const lum=hex=>{const rgb=hex.slice(1).match(/../g).map(v=>parseInt(v,16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return rgb[0]*.2126+rgb[1]*.7152+rgb[2]*.0722;};
const ratio=(a,b)=>{const values=[lum(a),lum(b)].sort((a,b)=>a-b);return (values[1]+.05)/(values[0]+.05);};
for(const [mode,theme] of Object.entries(tokens.theme)){
  const pairs=[['text','bg'],['text','surface'],['text','surfaceRaised'],['muted','surface'],['muted','bg'],['link','surface'],['actionText','action'],...['info','success','warning','danger'].map(v=>[v+'Text',v+'Bg'])];
  for(const [fg,bg] of pairs){const contrast=ratio(resolve(theme[fg]),resolve(theme[bg]));check(contrast>=4.5,`${mode}: ${fg}/${bg} contrast ${contrast.toFixed(2)} is below 4.5.`);}
  check(ratio(resolve(theme.focus),resolve(theme.surface))>=3,`${mode}: focus ring contrast is below 3.`);
}
const fontMagic=Buffer.from([0,1,0,0]);
for(const name of ['Sora-Variable.ttf','Inter-Variable.ttf','Inter-Italic-Variable.ttf']){const data=await readFile(path.join(root,'assets/fonts',name));check(data.subarray(0,4).equals(fontMagic)&&data.length>50000,`${name}: invalid TrueType font.`);}
const ico=await readFile(path.join(root,'assets/icons/app-icon.ico'));check(ico.readUInt16LE(2)===1&&ico.readUInt16LE(4)===7,'ICO: invalid header or missing sizes.');
for(let i=0;i<ico.readUInt16LE(4);i++){const p=6+i*16,size=ico.readUInt32LE(p+8),offset=ico.readUInt32LE(p+12);check(offset+size<=ico.length&&ico.subarray(offset,offset+8).equals(Buffer.from([137,80,78,71,13,10,26,10])),'ICO: invalid embedded PNG.');}
const icns=await readFile(path.join(root,'assets/icons/app-icon.icns'));check(icns.toString('ascii',0,4)==='icns'&&icns.readUInt32BE(4)===icns.length,'ICNS: invalid container.');
let offset=8,count=0;while(offset<icns.length){const size=icns.readUInt32BE(offset+4);check(size>=8&&offset+size<=icns.length,'ICNS: invalid chunk length.');if(size<8)break;offset+=size;count++;}check(count===7&&offset===icns.length,'ICNS: missing or malformed sizes.');
async function exists(ref,base){if(!ref||ref.startsWith('#')||/^(?:https?:|data:|mailto:)/.test(ref))return;const target=path.resolve(path.dirname(base),decodeURIComponent(ref.split(/[?#]/)[0]));try{await access(target);checks++;}catch{failures.push(`Missing reference ${ref} from ${path.relative(root,base)}.`);}}
const htmlPath=path.join(root,'index.html'),html=await readFile(htmlPath,'utf8');
for(const match of html.matchAll(/(?:src|href)="([^"]+)"/g))await exists(match[1],htmlPath);
for(const match of html.matchAll(/url\(['"]?([^)'"\s]+)['"]?\)/g))await exists(match[1],htmlPath);
for(const folder of ['styles','demo'])for(const name of await readdir(path.join(root,folder)))if(name.endsWith('.css')){const filename=path.join(root,folder,name),css=await readFile(filename,'utf8');for(const m of css.matchAll(/url\(['"]?([^)'"\s]+)['"]?\)/g))await exists(m[1],filename);for(const m of css.matchAll(/@import\s+['"]([^'"]+)['"]/g))await exists(m[1],filename);}
const allCss=(await Promise.all(['styles/tokens.css','styles/components.css','demo/reference.css'].map(p=>readFile(path.join(root,p),'utf8')))).join('\n');
const defined=new Set([...allCss.matchAll(/(--kl-[a-z0-9-]+)\s*:/g)].map(m=>m[1]));
for(const m of allCss.matchAll(/var\((--kl-[a-z0-9-]+)/g))check(defined.has(m[1]),`Undefined token: ${m[1]}`);
for(const file of ['assets/fonts/Sora-OFL.txt','assets/fonts/Inter-OFL.txt','assets/ui-icons/LICENSE'])await exists(file,path.join(root,'root-file'));
const manifest=JSON.parse(await readFile(path.join(root,'assets/manifest.json'),'utf8'));
for(const asset of manifest.files){const data=await readFile(path.join(root,'assets',asset.path));check(data.length===asset.bytes&&createHash('sha256').update(data).digest('hex')===asset.sha256,`Changed asset without rebuilding manifest: ${asset.path}`);}
if(failures.length){console.error(failures.join('\n'));process.exitCode=1;}else console.log(`Passed ${checks} checks: theme contrast, local references, font integrity, icon containers, and CSS tokens.`);
