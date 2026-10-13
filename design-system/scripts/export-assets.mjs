import sharp from 'sharp';
import { readFile, writeFile, mkdir, copyFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const root = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
const source = path.join(root, 'assets/source/kitlane-original.png');
const logoDir = path.join(root, 'assets/logos');
const iconDir = path.join(root, 'assets/icons');
await Promise.all([mkdir(logoDir, { recursive: true }), mkdir(iconDir, { recursive: true }), mkdir(path.join(root, 'assets/ui-icons'), { recursive: true }),mkdir(path.join(root,'assets/brand'),{recursive:true})]);

// Trace pixel boundaries from the approved artwork, preserving the actual lettering.
// RDP reduces staircase points without inventing replacement logo geometry.
function simplify(points, epsilon = 0.9) {
  if (points.length < 3) return points;
  const a = points[0], b = points.at(-1);
  let max = 0, at = 0;
  for (let i = 1; i < points.length - 1; i++) {
    const p = points[i], dx = b[0] - a[0], dy = b[1] - a[1];
    const t = Math.max(0, Math.min(1, ((p[0]-a[0])*dx + (p[1]-a[1])*dy) / (dx*dx+dy*dy || 1)));
    const d = Math.hypot(p[0]-a[0]-t*dx, p[1]-a[1]-t*dy);
    if (d > max) { max = d; at = i; }
  }
  return max > epsilon ? [...simplify(points.slice(0, at+1), epsilon).slice(0, -1), ...simplify(points.slice(at), epsilon)] : [a,b];
}
function outline(mask, width, height) {
  const edges = new Map();
  const add = (x,y,nx,ny) => { const key = `${x},${y}`; if (!edges.has(key)) edges.set(key, []); edges.get(key).push([nx,ny]); };
  const has = (x,y) => x>=0 && y>=0 && x<width && y<height && mask[y*width+x];
  for (let y=0;y<height;y++) for (let x=0;x<width;x++) if (has(x,y)) {
    if (!has(x,y-1)) add(x,y,x+1,y);
    if (!has(x+1,y)) add(x+1,y,x+1,y+1);
    if (!has(x,y+1)) add(x+1,y+1,x,y+1);
    if (!has(x-1,y)) add(x,y+1,x,y);
  }
  const paths = [];
  while (edges.size) {
    const start = edges.keys().next().value.split(',').map(Number);
    const points = [start]; let p = start;
    for (let guard=0;guard<width*height*4;guard++) {
      const key = p.join(','), next = edges.get(key);
      if (!next?.length) break;
      p = next.pop(); if (!next.length) edges.delete(key);
      points.push(p); if (p[0]===start[0] && p[1]===start[1]) break;
    }
    if (points.length < 12) continue;
    const half = Math.floor(points.length/2);
    const reduced = [...simplify(points.slice(0,half+1)), ...simplify(points.slice(half)).slice(1)];
    if(reduced.at(-1)[0]===reduced[0][0]&&reduced.at(-1)[1]===reduced[0][1]) reduced.pop();
    const mid=(a,b)=>[(a[0]+b[0])/2,(a[1]+b[1])/2];
    paths.push(`M${mid(reduced.at(-1),reduced[0]).join(' ')}${reduced.map((p,i)=>`Q${p.join(' ')} ${mid(p,reduced[(i+1)%reduced.length]).join(' ')}`).join('')}Z`);
  }
  return paths.join('');
}
async function trace(crop, classify, palette) {
  const { data, info } = await sharp(source).extract(crop).blur(0.7).removeAlpha().raw().toBuffer({resolveWithObject:true});
  const masks = palette.map(()=>new Uint8Array(info.width*info.height));
  for (let i=0;i<info.width*info.height;i++) {
    const c = classify(data[i*3],data[i*3+1],data[i*3+2],i%info.width,Math.floor(i/info.width));
    if (c>=0) masks[c][i]=1;
  }
  return { width:info.width, height:info.height, layers:masks.map((m,i)=>({fill:palette[i], d:outline(m,info.width,info.height)})) };
}
const palette = ['#091C31','#01BCFB','#19F5BE'];
const logoClass = (r,g,b) => b<115 && r<100 ? 0 : b>150 && r<110 && b>g*1.12 ? 1 : g>150 && r<110 && g>b*1.03 ? 2 : -1;
const lockup = await trace({left:201,top:207,width:1093,height:253},logoClass,palette);
const mark = await trace({left:201,top:207,width:285,height:253},logoClass,palette);
const wordmark = await trace({left:533,top:249,width:761,height:184},logoClass,palette);
const icon = await trace({left:526,top:552,width:397,height:374},(r,g,b,x,y)=>b<115&&r<100?0:b>150&&r<110&&b>g*1.12?1:g>150&&r<110&&g>b*1.03?2:r>190&&g>190&&b>190&&x>70&&x<330&&y>70&&y<320?3:-1,['#081B31','#01BCFB','#19F5BE','#FFFFFF']);
function svg(asset, overrides={}, square=false) {
  const size = square ? Math.max(asset.width,asset.height) : null;
  const x = square ? (size-asset.width)/2 : 0, y=square?(size-asset.height)/2:0;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size||asset.width} ${size||asset.height}" role="img"><title>Kitlane</title><g transform="translate(${x} ${y})">${asset.layers.filter(l=>l.d).map((l,i)=>`<path fill="${overrides[i]||l.fill}" fill-rule="evenodd" d="${l.d}"/>`).join('')}</g></svg>`;
}
const variants = {
  'kitlane-logo':svg(lockup), 'kitlane-logo-reversed':svg(lockup,{0:'#FFFFFF'}),
  'kitlane-logo-monochrome':svg(lockup,{0:'#081B31',1:'#081B31',2:'#081B31'}),
  'kitlane-logo-monochrome-white':svg(lockup,{0:'#FFFFFF',1:'#FFFFFF',2:'#FFFFFF'}),
  'kitlane-mark':svg(mark), 'kitlane-mark-reversed':svg(mark,{0:'#FFFFFF'}),
  'kitlane-mark-monochrome':svg(mark,{0:'#081B31',1:'#081B31',2:'#081B31'}),
  'kitlane-wordmark':svg(wordmark), 'kitlane-wordmark-reversed':svg(wordmark,{0:'#FFFFFF'})
};
for (const [name,body] of Object.entries(variants)) {
  await writeFile(path.join(logoDir,name+'.svg'),body);
  await sharp(Buffer.from(body)).resize(name.includes('mark')&&!name.includes('wordmark')?512:1600).png().toFile(path.join(logoDir,name+'.png'));
}
const appSvg=svg(icon,{},true);
await writeFile(path.join(iconDir,'app-icon.svg'),appSvg);
for (const size of [16,24,32,48,64,128,256,512,1024]) await sharp(Buffer.from(appSvg)).resize(size,size).png().toFile(path.join(iconDir,`app-icon-${size}.png`));
const traySvg=svg(mark,{0:'#081B31',1:'#081B31',2:'#081B31'},true);
const trayWhite=svg(mark,{0:'#FFFFFF',1:'#FFFFFF',2:'#FFFFFF'},true);
await writeFile(path.join(iconDir,'tray-icon.svg'),traySvg);
await writeFile(path.join(iconDir,'tray-icon-white.svg'),trayWhite);
for (const size of [16,24,32]) for(const [name,body] of [['tray-icon',traySvg],['tray-icon-white',trayWhite]]) await sharp(Buffer.from(body)).resize(size,size).png().toFile(path.join(iconDir,`${name}-${size}.png`));
const icoSizes=[16,24,32,48,64,128,256], icoData=await Promise.all(icoSizes.map(s=>readFile(path.join(iconDir,`app-icon-${s}.png`))));
const icoHead=Buffer.alloc(6+icoSizes.length*16);icoHead.writeUInt16LE(1,2);icoHead.writeUInt16LE(icoSizes.length,4);let offset=icoHead.length;
icoData.forEach((data,i)=>{const n=6+i*16;icoHead[n]=icoSizes[i]===256?0:icoSizes[i];icoHead[n+1]=icoHead[n];icoHead.writeUInt16LE(1,n+4);icoHead.writeUInt16LE(32,n+6);icoHead.writeUInt32LE(data.length,n+8);icoHead.writeUInt32LE(offset,n+12);offset+=data.length;});
await writeFile(path.join(iconDir,'app-icon.ico'),Buffer.concat([icoHead,...icoData]));
const icns=[];for(const [type,size] of [['icp4',16],['icp5',32],['icp6',64],['ic07',128],['ic08',256],['ic09',512],['ic10',1024]]){const data=await readFile(path.join(iconDir,`app-icon-${size}.png`));const head=Buffer.alloc(8);head.write(type);head.writeUInt32BE(data.length+8,4);icns.push(head,data);}
const icnsHead=Buffer.alloc(8);icnsHead.write('icns');icnsHead.writeUInt32BE(8+icns.reduce((n,b)=>n+b.length,0),4);await writeFile(path.join(iconDir,'app-icon.icns'),Buffer.concat([icnsHead,...icns]));
for(const name of ['check','arrow-up','arrow-right','refresh-cw','monitor','laptop','download','search','x','chevron-down','chevron-right','settings','sun','moon','circle-alert','shield-check','circle-slash','copy','external-link','package','plus','cloud','info','check-check','loader-circle','minus','square']) await copyFile(path.join(root,'node_modules/lucide-static/icons',name+'.svg'),path.join(root,'assets/ui-icons',name+'.svg'));
await copyFile(path.join(root,'node_modules/lucide-static/LICENSE'),path.join(root,'assets/ui-icons/LICENSE'));
const markPaths=mark.layers.map(l=>`<path fill="${l.fill}" fill-rule="evenodd" d="${l.d}"/>`).join('');
const pattern=`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 600"><title>Kitlane modular lane pattern</title><defs><pattern id="lane" width="240" height="220" patternUnits="userSpaceOnUse"><g transform="translate(20 18) scale(.6)" opacity=".16">${markPaths}</g></pattern></defs><rect width="1200" height="600" fill="#F4F7FA"/><rect width="1200" height="600" fill="url(#lane)"/></svg>`;
await writeFile(path.join(root,'assets/brand/brand-pattern.svg'),pattern);
const cover=`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 630"><title>Kitlane brand cover</title><rect width="1200" height="630" fill="#081B31"/><g transform="translate(110 210) scale(.9)">${lockup.layers.map((l,i)=>`<path fill="${i===0?'#FFFFFF':l.fill}" fill-rule="evenodd" d="${l.d}"/>`).join('')}</g><rect x="110" y="485" width="62" height="6" rx="3" fill="#01BCFB"/><rect x="182" y="485" width="62" height="6" rx="3" fill="#19F5BE"/></svg>`;
await writeFile(path.join(root,'assets/brand/brand-cover.svg'),cover);await sharp(Buffer.from(cover)).png().toFile(path.join(root,'assets/brand/brand-cover.png'));
const {readdir}=await import('node:fs/promises');
const files=[];async function inventory(dir){for(const entry of await readdir(dir,{withFileTypes:true})){const p=path.join(dir,entry.name);if(entry.isDirectory())await inventory(p);else if(entry.name!=='manifest.json'){const bytes=await readFile(p);files.push({path:path.relative(path.join(root,'assets'),p).replaceAll('\\','/'),bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')});}}}
await inventory(path.join(root,'assets'));
await writeFile(path.join(root,'assets/manifest.json'),JSON.stringify({version:'0.1.0',logoSource:'source/kitlane-original.png',fontSources:{Sora:'https://github.com/google/fonts/tree/main/ofl/sora',Inter:'https://github.com/google/fonts/tree/main/ofl/inter'},interfaceIcons:'Lucide / ISC',files},null,2)+'\n');
console.log(`Exported logo variants, desktop icons, brand artwork, and manifest with ${files.length} assets.`);
