import { setTheme, initTabs, toast } from '../src/index.js';
let themeCleanup;
const saved=(()=>{try{return localStorage.getItem('kitlane-theme')||'light';}catch{return 'light';}})();
function chooseTheme(mode) {
  themeCleanup?.(); themeCleanup=setTheme(mode);
  document.querySelectorAll('[data-theme-choice]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.themeChoice===mode)));
  try{localStorage.setItem('kitlane-theme',mode);}catch{}
}
chooseTheme(['light','dark','system'].includes(saved)?saved:'light');
document.querySelectorAll('[data-theme-choice]').forEach(b=>b.addEventListener('click',()=>chooseTheme(b.dataset.themeChoice)));
initTabs(document.getElementById('platform-tabs'));
document.querySelectorAll('[data-notify]').forEach(b=>b.addEventListener('click',()=>toast(b.dataset.notify)));
document.querySelectorAll('[data-copy]').forEach(b=>b.addEventListener('click',async()=>{
  try{await navigator.clipboard.writeText(b.dataset.copy);toast(`Copied ${b.dataset.copy}.`);}catch{toast('Clipboard access is unavailable. Select the hex value to copy it.',{tone:'info'});}
}));
const weight=document.getElementById('weight');weight.addEventListener('input',()=>{document.getElementById('sora-specimen').style.fontWeight=weight.value;document.getElementById('weight-label').textContent=weight.value;});
const dialog=document.getElementById('review-dialog');
let dialogTrigger;
document.querySelectorAll('[data-open-dialog]').forEach(b=>b.addEventListener('click',()=>{dialogTrigger=b;dialog.showModal();}));
dialog.addEventListener('close',()=>{dialogTrigger?.focus();if(dialog.returnValue==='update')toast('Update queued. This is a design-system example.');});
const loading=document.getElementById('loading-button');
loading.addEventListener('click',()=>{loading.disabled=true;loading.setAttribute('aria-busy','true');loading.querySelector('span:last-child').textContent='Syncing…';setTimeout(()=>{loading.disabled=false;loading.removeAttribute('aria-busy');loading.querySelector('span:last-child').textContent='Sync setup';toast('Your app profile is synced.');},1600);});
document.getElementById('progress-button').addEventListener('click',event=>{const button=event.currentTarget;button.disabled=true;let value=0;const progress=document.getElementById('sample-progress'),label=document.getElementById('progress-value');const update=()=>{progress.value=value;progress.textContent=`${value}%`;label.textContent=`${value}%`;};update();const interval=setInterval(()=>{value=Math.min(100,value+10);update();if(value===100){clearInterval(interval);button.disabled=false;toast('Progress example complete.');}},180);});
const observer=new IntersectionObserver(entries=>{for(const entry of entries)if(entry.isIntersecting){document.querySelectorAll('.section-nav a').forEach(a=>{if(a.hash===`#${entry.target.id}`)a.setAttribute('aria-current','location');else a.removeAttribute('aria-current');});}},{rootMargin:'-15% 0px -65% 0px'});
document.querySelectorAll('main>.section').forEach(section=>observer.observe(section));
