const key='kitlane.pending-sign-in';
const valid=(value:unknown):value is string=>typeof value==='string'&&/^[A-Za-z0-9_-]{43}$/.test(value);

// Managed OAuth can return to the trusted origin root. Retain only the short-lived
// attempt identifier in this tab; it cannot authorize a device without Google + PKCE.
export function pendingAttempt(url:URL,storage:Pick<Storage,'getItem'|'setItem'|'removeItem'>,now=Date.now()):string|null {
  const supplied=url.searchParams.get('attempt');
  try {
    if(supplied!==null){
      if(!valid(supplied))return null;
      storage.setItem(key,JSON.stringify({id:supplied,expires:now+300_000}));
      return supplied;
    }
    const cached=JSON.parse(storage.getItem(key)??'null');
    if(valid(cached?.id)&&Number.isFinite(cached.expires)&&cached.expires>now&&cached.expires<=now+300_000)return cached.id;
    storage.removeItem(key);
  }catch {/* Blocked storage leaves the normal callback path usable. */}
  return valid(supplied)?supplied:null;
}
