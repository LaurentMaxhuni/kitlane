import {createAuthClient} from '@neondatabase/auth';
import {pendingAttempt} from './pending-attempt.js';
import {googleToken} from './google-token.js';

const google=document.querySelector<HTMLButtonElement>('#google')!;
const authorize=document.querySelector<HTMLButtonElement>('#authorize')!;
const status=document.querySelector<HTMLElement>('#status')!;
const description=document.querySelector<HTMLElement>('#description')!;
let attempt=new URL(location.href).searchParams.get('attempt');
try{attempt=pendingAttempt(new URL(location.href),sessionStorage);}catch{/* Browser storage may be disabled. */}
async function setup(){
  if(!attempt||!/^[A-Za-z0-9_-]{43}$/.test(attempt))throw new Error();
  const [configResult,attemptResult]=await Promise.all([fetch('/v2/config'),fetch('/v2/auth/attempts/'+attempt)]);
  if(!configResult.ok||!attemptResult.ok)throw new Error();
  const config=await configResult.json(),details=await attemptResult.json();
  const auth=createAuthClient(config.auth_url);
  const callback=location.origin+'/sign-in?attempt='+attempt;
  google.disabled=false;status.textContent='Continue in Google, then authorize the computer shown here.';
  description.textContent='Connect “'+details.device.name+'” to your Google account.';
  google.onclick=async()=>{
    google.disabled=true;status.textContent='Opening Google…';
    try{const result=await auth.signIn.social({provider:'google',callbackURL:callback});if(result.error)throw new Error();}
    catch{google.disabled=false;status.textContent='Google sign-in could not start. Try again.';}
  };
  const current=await auth.getSession();
  if(current.data?.user){
    google.textContent='Use another Google account';
    google.onclick=async()=>{await auth.signOut();await auth.signIn.social({provider:'google',callbackURL:callback});};
    authorize.hidden=false;
    description.textContent='Authorize “'+details.device.name+'” for '+current.data.user.email+'. Only authorize a sign-in you started in Kitlane.';
    status.textContent='Confirm to return to Kitlane.';
    authorize.onclick=async()=>{
      authorize.disabled=true;
      try{
        const token=await googleToken(auth);
        const result=await fetch('/v2/auth/complete',{method:'POST',headers:{'content-type':'application/json',authorization:'Bearer '+token},body:JSON.stringify({attempt})});
        if(!result.ok)throw new Error();
        const body=await result.json(),url=new URL(body.callback);
        if(url.protocol!=='http:'||url.hostname!=='127.0.0.1'||url.pathname!=='/kitlane/callback')throw new Error();
        try{sessionStorage.removeItem('kitlane.pending-sign-in');}catch{/* No credentials are stored here. */}
        status.textContent='Returning to Kitlane…';location.assign(url.href);
      }catch{authorize.disabled=false;status.textContent='Authorization failed or expired. Start a new sign-in from Kitlane.';}
    };
  }
}
setup().catch(()=>{google.disabled=true;status.textContent='This sign-in attempt expired or is unavailable. Start again from Kitlane.';});
