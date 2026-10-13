import {build} from 'esbuild';
import {readFile,mkdir} from 'node:fs/promises';

const browser=await build({entryPoints:['web/sign-in.ts'],bundle:true,write:false,format:'esm',platform:'browser',target:'es2022',minify:true});
const html=await readFile('web/sign-in.html','utf8');
await mkdir('dist',{recursive:true});
await build({entryPoints:['src/index.ts'],bundle:true,outfile:'dist/index.mjs',format:'esm',platform:'node',target:'node24',banner:{js:"import {createRequire} from 'node:module'; const require=createRequire(import.meta.url);"},plugins:[{name:'bridge-assets',setup(builder){builder.onLoad({filter:/src[\\/]assets\.ts$/},()=>({contents:`export const html=${JSON.stringify(html)}; export const script=${JSON.stringify(browser.outputFiles![0]!.text)};`,loader:'ts'}));}}]});
