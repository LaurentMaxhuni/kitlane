export async function googleToken(auth:{getSession:()=>Promise<{data:unknown;error:unknown}>}):Promise<string> {
  // The pinned SDK injects set-auth-jwt into session.token. Its /token call
  // shares getSession's cache and can instead return a session-shaped response.
  const {data,error}=await auth.getSession();
  if(error||!data||typeof data!=='object'||!('user' in data)||!data.user||!('session' in data))throw new Error('Google session unavailable');
  const session=data.session;
  const token=session&&typeof session==='object'&&'token' in session?session.token:null;
  if(typeof token!=='string'||token.length>16384||!/^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.test(token))throw new Error('Google session unavailable');
  return token;
}
