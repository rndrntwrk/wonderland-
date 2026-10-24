// Native account transport. Owned lot readiness requires a committed server grant.
// Legacy Aries sessions and browser-supplied actor/principal authority are never synthesized.
import {loadNativeContent} from './native-content.mjs';
export function createNativeAccountProvider(base, publish, options = {}) {
  const origin = options.origin ?? globalThis.location.origin;
  const endpoint = new URL(base, origin);
  if (
    (endpoint.protocol !== "https:" &&
      !(
        endpoint.protocol === "http:" &&
        ["localhost", "127.0.0.1", "[::1]"].includes(endpoint.hostname)
      )) ||
    endpoint.origin !== origin ||
    endpoint.pathname !== "/native-account" ||
    endpoint.search ||
    endpoint.hash ||
    endpoint.username ||
    endpoint.password
  )
    throw Error("Invalid native account endpoint");
  const fetcher = options.fetch ?? globalThis.fetch;
  const nativeLots = options.nativeLots === true;
  const contentRequests = new Set();
  let destinations = [], admissionAvailable = false, owned = null, pendingEntry = null, firstTicket = null, detachPending = false, socketRequest = null;
  let recoverySerial = 0, placementPending = false, placementSerial = 0;
  const operationId = options.operationId ?? (() => globalThis.crypto.randomUUID().replaceAll("-", ""));
  const decimal = v => typeof v === "string" && /^[1-9]\d{0,19}$/.test(v) && BigInt(v) <= 18446744073709551615n;
  let token = null,
    generation = 0,
    active = null,
    disposed = false,
    avatars = [];
  let creator=null,pendingCreation=null,creationUnknown=false,createdAvatarId=null;
  const digest=v=>typeof v==='string'&&/^[0-9a-f]{64}$/.test(v);
  const emit = (value) => {
    if (!disposed) publish({creator_open:!!creator,creation_options:creator?structuredClone(creator):null,creation_pending:!!pendingCreation,creation_unknown:creationUnknown,created_avatar_id:createdAvatarId, destinations: token ? destinations : [], admission_available: !!token && admissionAvailable, ...value });
  };
  async function request(action, data, secret, signal) {
    const r = await fetcher(endpoint.href + "/" + action, {
      method: "POST",
      credentials: "omit",
      cache: "no-store",
      redirect: "error",
      headers: {
        "Content-Type": "application/json",
        ...(secret ? { Authorization: "Bearer " + secret } : {}),
      },
      body: JSON.stringify(data),
      signal,
    });
    if (!r.ok) {
      const error = Error(
        r.status === 401
          ? "Sign in again to access your account."
          : "The native account service is unavailable.",
      );
      error.unauthorized = r.status === 401;
      error.status = r.status;
      throw error;
    }
    if (!r.headers.get("content-type")?.startsWith("application/json"))
      throw Error("Invalid account response.");
    const reader = r.body?.getReader();
    if (!reader) throw Error("Invalid account response.");
    let size = 0;
    const chunks = [];
    try {
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.length;
        if (size > (action === 'eod' ? 512 * 1024 : action === 'chat' ? 128 * 1024 : 65536)) throw Error("Account response exceeds its limit.");
        chunks.push(value);
      }
      const bytes = new Uint8Array(size);
      let at = 0;
      for (const c of chunks) {
        bytes.set(c, at);
        at += c.length;
      }
      try {
        return JSON.parse(
          new TextDecoder("utf-8", { fatal: true }).decode(bytes),
        );
      } catch {
        throw Error("Invalid account response.");
      }
    } finally {
      void reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  }
  function begin(action) {
    if (disposed || active) return null;
    const n = ++generation;
    for (const controller of contentRequests) controller.abort();
    const controller = new AbortController();
    active = { controller, action };
    emit({
      state:
        action === "logout"
          ? "signing_out"
          : token
            ? "account_refreshing"
            : "loading",
      avatars: token ? avatars : [],
      message:
        action === "logout"
          ? "Signing out…"
          : token
            ? "Refreshing your Sims…"
            : "Connecting to your native account…",
    });
    return {
      n,
      controller,
      timer: setTimeout(() => controller.abort(), 15000),
    };
  }
  const current = (job) => !disposed && generation === job.n;
  const finish = (job) => {
    clearTimeout(job.timer);
    if (current(job)) active = null;
  };
  function validateRoster(value) {
    if (
      value.version !== 1 ||
      value.state !== "account_ready" ||
      (value.admission_available !== false && !(nativeLots && value.admission_available === true)) ||
      !/^\d+$/.test(value.account_id) ||
      !Array.isArray(value.avatars) ||
      value.avatars.length > 3 ||
      value.next !== null
    )
      throw Error("Unsupported account roster.");
    return value.avatars.map((v) => {
      if (
        typeof v.avatar_id !== "string" ||
        !/^[1-9]\d{0,7}$/.test(v.avatar_id) ||
        Number(v.avatar_id) > 16777215 ||
        typeof v.name !== "string" ||
        v.name.length > 24
      )
        throw Error("Invalid avatar record.");
      return { avatar_id: v.avatar_id, name: v.name };
    });
  }
  async function roster(job) {
    const v = await request(
      "roster",
      { after: "0" },
      token,
      job.controller.signal,
    );
    const nextAvatars = validateRoster(v);
    if (current(job)) {
      const nextDestinations = v.admission_available === true ? validateDestinations(v.destinations) : [];
      avatars = nextAvatars;
      destinations = nextDestinations;
      admissionAvailable = nativeLots && v.admission_available === true;
      emit({
        state: "account_ready",
        avatars,
        message:
          admissionAvailable ? "Choose your Sim and a property." : "Account connected. Native world admission is not yet available.",
      });
    }
  }
  async function login(username, password) {
    if (token) return;
    const job = begin("login");
    if (!job) return;
    token = null;
    avatars = [];
    try {
      const v = await request(
        "login",
        { username, password },
        null,
        job.controller.signal,
      );
      password = "";
      if (!current(job)) return;
      if (
        v.version !== 1 ||
        v.state !== "account_ready" ||
        (v.admission_available !== false && !(nativeLots && v.admission_available === true)) ||
        typeof v.token !== "string" ||
        !/^[0-9a-f]{64}$/.test(v.token)
      )
        throw Error("Invalid account session.");
      token = v.token;
      await roster(job);
    } catch (error) {
      if (current(job)) {
        if (error.unauthorized) {
          token = null;
          avatars = [];
        }
        emit({
          state: token ? "account_error" : "signed_out",
          avatars: [],
          message:
            error.name === "AbortError"
              ? token
                ? "Connection timed out. Refresh your roster or sign out."
                : "Connection timed out. Please sign in again."
              : error.message,
        });
      }
    } finally {
      password = "";
      finish(job);
    }
  }
  async function refresh() {
    if (owned || pendingEntry || detachPending) return;
    const job = begin("refresh");
    if (!job) return;
    if (!token) {
      finish(job);
      emit({
        state: "signed_out",
        avatars: [],
        message: "Sign in to access your account.",
      });
      return;
    }
    try {
      await roster(job);
    } catch (error) {
      if (current(job)) {
        if (error.unauthorized) {
          token = null;
          avatars = [];
        }
        emit({
          state: token ? "account_error" : "signed_out",
          avatars: [],
          message: error.message,
        });
      }
    } finally {
      finish(job);
    }
  }

  async function openCreator() {
    if(!nativeLots||!token||owned||pendingEntry||detachPending||disposed||active||pendingCreation)return;
    const job=begin("creator");if(!job)return;
    try{
      let after="0",release=null,slots=null;const entries=[],seen=new Set();
      for(let page=0;page<257;page++){
        const value=await request("creation-options",{after},token,job.controller.signal);
        if(!current(job))return;
        if(value.version!==1||value.state!=="creation_options"||!digest(value.release)
          ||!Number.isSafeInteger(value.available_slots)||value.available_slots<0||value.available_slots>3
          ||!Array.isArray(value.entries)||value.entries.length>64)throw Error("Invalid creation catalog.");
        if(release!==null&&(value.release!==release||value.available_slots!==slots))throw Error("Creation catalog changed. Open the creator again.");
        release=value.release;slots=value.available_slots;
        for(const row of value.entries){
          if(!decimal(row.key)||seen.has(row.key)||!["head","body"].includes(row.part)||!["male","female"].includes(row.gender)
            ||!Array.isArray(row.skins)||!row.skins.length||row.skins.length>3||new Set(row.skins).size!==row.skins.length
            ||!row.skins.every(s=>["light","medium","dark"].includes(s)))throw Error("Invalid creation appearance.");
          seen.add(row.key);entries.push({...row,skins:row.skins.slice()});
        }
        if(entries.length>16384)throw Error("Creation catalog exceeds its resource budget.");
        if(value.next===null)break;
        if(!decimal(value.next)||BigInt(value.next)!==BigInt(after)+BigInt(value.entries.length)||!value.entries.length||page===256)
          throw Error("Invalid creation catalog pagination.");
        after=value.next;
      }
      if(!entries.some(e=>e.part==="head")||!entries.some(e=>e.part==="body"))throw Error("Character appearance is unavailable.");
      creator={release,available_slots:slots,entries};creationUnknown=false;
      emit({state:"account_ready",avatars,message:slots?"Choose your Sim's original appearance.":"This account already has three Sims."});
    }catch(error){if(current(job)){
      creator=null;if(error.unauthorized){token=null;avatars=[];}
      emit({state:token?"account_error":"signed_out",avatars,message:error.message});
    }}
    finally{finish(job);}
  }
  function closeCreator(){
    if(active||pendingCreation||disposed)return;
    for(const controller of contentRequests)controller.abort();
    creator=null;generation++;emit({state:token?"account_ready":"signed_out",avatars,message:"Choose your Sim and a property."});
  }
  function eligibleSpec(spec){
    if(!creator||!spec||typeof spec!=="object"||Array.isArray(spec)
      ||Object.keys(spec).sort().join(",")!=="body_key,description,gender,head_key,name,release,skin"
      ||spec.release!==creator.release||!creator.available_slots
      ||typeof spec.name!=="string"||!/^[a-zA-Z][a-zA-Z ]{2,23}$/.test(spec.name)
      ||typeof spec.description!=="string"||spec.description.length>499
      ||!["male","female"].includes(spec.gender)||!["light","medium","dark"].includes(spec.skin))
      throw Error("Choose an available original appearance and a valid name.");
    for(const part of ["head","body"])
      if(!creator.entries.some(e=>e.part===part&&e.key===spec[part+"_key"]&&e.gender===spec.gender&&e.skins.includes(spec.skin)))
        throw Error("This appearance is not eligible for character creation.");
    return {name:spec.name,description:spec.description,gender:spec.gender,skin:spec.skin,head_key:spec.head_key,body_key:spec.body_key,release:spec.release};
  }
  async function createAvatar(spec){
    if(!token||!creator||owned||pendingEntry||active||disposed)return;
    let avatar;
    try{avatar=eligibleSpec(spec);}catch(error){emit({state:"account_error",avatars,message:error.message});return;}
    if(pendingCreation&&JSON.stringify(pendingCreation.avatar)!==JSON.stringify(avatar)){
      emit({state:"account_error",avatars,message:"The previous creation is unconfirmed. Retry its exact request first."});return;
    }
    if(!pendingCreation){const operation=operationId();
      if(typeof operation!=="string"||!/^[a-zA-Z0-9_-]{16,96}$/.test(operation))throw Error("Invalid creation operation.");
      pendingCreation={operation,avatar};}
    return sendCreation();
  }
  async function retryCreate(){if(token&&creator&&pendingCreation&&!active&&!disposed)return sendCreation();}
  async function sendCreation(){
    const job=begin("create");if(!job)return;const submitted=pendingCreation;creationUnknown=false;
    emit({state:"account_refreshing",avatars,message:"Creating your Sim…"});
    try{
      const value=await request("create",submitted,token,job.controller.signal);
      if(!current(job))return;
      if(value.version!==1||value.state!=="avatar_created"||!decimal(value.avatar?.avatar_id)||BigInt(value.avatar.avatar_id)>16777215n
        ||Object.entries(submitted.avatar).some(([key,v])=>value.avatar[key]!==v))throw Error("Invalid creation receipt.");
      createdAvatarId=value.avatar.avatar_id;pendingCreation=null;creator=null;
      await roster(job);createdAvatarId=null;
    }catch(error){if(current(job)){
      if(error.unauthorized){token=null;avatars=[];creator=null;pendingCreation=null;}
      else if(error.status===400){pendingCreation=null;}
      creationUnknown=!!pendingCreation;
      emit({state:token?"account_error":"signed_out",avatars,
        message:pendingCreation?"Creation was not confirmed. Retry the same request; another Sim will not be requested.":error.message});
    }}finally{finish(job);}
  }

  function validateDestinations(value) {
    if (!Array.isArray(value) || value.length > 128) throw Error("Invalid property directory.");
    const seen = new Set();
    return value.map(v => {
      if (!decimal(v.lot_id) || seen.has(v.lot_id) || !Number.isInteger(v.lot_location) || v.lot_location < 1 || v.lot_location > 4294967295 || typeof v.name !== "string" || !v.name.length || v.name.length > 96 || /[\u0000-\u001f]/.test(v.name)) throw Error("Invalid property record.");
      seen.add(v.lot_id);
      return { lot_id: v.lot_id, lot_location: v.lot_location, name: v.name };
    });
  }
  function validateGrant(value, selection, expected) {
    const b = value?.binding;
    const waiting = value?.state === "admitted_awaiting_placement";
    if (value?.version !== 1 || (!waiting && value.state !== "lot_ready") || !b || !decimal(b.source_epoch) || !decimal(b.lot_incarnation) || b.avatar_id !== Number(selection.avatar_id) || b.lot_location !== selection.lot_location || (waiting ? (value.ticket !== undefined || value.blocker !== "source_placement_unavailable") : (typeof value.ticket !== "string" || !/^[a-f0-9]{48,128}$/.test(value.ticket)))) throw Error("The property did not return a valid owned admission.");
    const binding = { source_epoch: b.source_epoch, lot_incarnation: b.lot_incarnation, avatar_id: b.avatar_id, lot_location: b.lot_location };
    if (expected && (binding.source_epoch !== expected.source_epoch || binding.lot_incarnation !== expected.lot_incarnation || binding.avatar_id !== expected.avatar_id || binding.lot_location !== expected.lot_location)) throw Error("The property control changed. Return to your account before entering again.");
    return { binding, ticket: value.ticket, waiting };
  }
  async function enter(avatarId, lotId) {
    if (!nativeLots || !token || !admissionAvailable || owned || detachPending || active || disposed || creator || pendingCreation) return;
    const avatar = avatars.find(v => v.avatar_id === avatarId);
    const lot = destinations.find(v => v.lot_id === lotId);
    if (!avatar || !lot) return;
    if (pendingEntry && (pendingEntry.avatar_id !== avatarId || pendingEntry.lot_id !== lotId)) {
      emit({state:"account_error",avatars,message:"The previous entry is still unconfirmed. Retry that same Sim and property or sign out."}); return;
    }
    if (!pendingEntry) {
      const operation = operationId();
      if (typeof operation !== "string" || !/^[a-zA-Z0-9_-]{16,96}$/.test(operation)) throw Error("Invalid entry operation.");
      pendingEntry = {avatar_id:avatarId,lot_id:lotId,lot_location:lot.lot_location,lot_name:lot.name,operation};
    }
    const selection = pendingEntry;
    const job = begin("enter"); if (!job) return;
    emit({state:"lot_entering",avatars,message:"Entering your property…"});
    try {
      const value = await request("enter", {avatar_id:avatarId,lot_id:lotId,operation:selection.operation}, token, job.controller.signal);
      if (!current(job)) return;
      const grant = validateGrant(value, selection);
      if (!Number.isSafeInteger(++recoverySerial)) throw Error("Account recovery scope exhausted.");
      owned = {...selection,binding:grant.binding,recovery_scope:String(recoverySerial)}; firstTicket = grant.ticket ?? null; pendingEntry = null;
      if (grant.waiting) { placementUnavailable(); return; }
      placementPending = false;
      emit({state:"lot_ready",avatars,grant:{binding:owned.binding,lot_id:owned.lot_id,lot_name:owned.lot_name,recovery_scope:owned.recovery_scope},message:"Property admission committed."});
    } catch(error) {
      if (current(job)) {
        if(error.unauthorized){token=null;avatars=[];pendingEntry=null;}
        emit({state:token?"account_error":"signed_out",avatars,message:error.unauthorized?error.message:"Property entry was not confirmed. Retry this same selection; a second actor will not be requested."});
      }
    } finally {finish(job);}
  }
  async function socketGrant() {
    if (!token || !owned || detachPending || disposed || active || socketRequest) throw Error("No active owned property admission.");
    if (firstTicket) {const ticket=firstTicket;firstTicket=null;return {binding:owned.binding,ticket};}
    const n=generation, placementEpoch=placementSerial, selection=owned, controller=new AbortController();
    socketRequest=controller;
    const timeout=setTimeout(()=>controller.abort(),15000);
    try {
      const result=await request("reconnect",{avatar_id:selection.avatar_id,lot_id:selection.lot_id,operation:selection.operation},token,controller.signal);
      if(disposed || generation!==n || placementSerial!==placementEpoch || owned!==selection || detachPending) throw Error("This account connection changed.");
      const grant=validateGrant(result,selection);
      if(grant.binding.source_epoch!==selection.binding.source_epoch || BigInt(grant.binding.lot_incarnation)<BigInt(selection.binding.lot_incarnation))throw Error("The property control changed.");
      if(grant.waiting){owned={...selection,binding:grant.binding};placementUnavailable();throw Error("Your Sim is admitted but awaiting source placement.");}
      if(grant.binding.lot_incarnation!==selection.binding.lot_incarnation){
        // A verified same-account grant may advance only the host incarnation.
        // Publish a new scope so WASM discards its old replica and bootstraps
        // afresh; never feed a new authority into an old player binding.
        owned={...selection,binding:grant.binding};firstTicket=grant.ticket;socketRequest=null;placementPending=false;
        emit({state:"lot_ready",avatars,grant:{binding:owned.binding,lot_id:owned.lot_id,lot_name:owned.lot_name,recovery_scope:owned.recovery_scope},message:"The property recovered. Reconnecting your Sim…"});
        throw Error("The property recovered; a fresh native bootstrap is required.");
      }
      // Keep the public socket DTO identical to the first-ticket path.
      // Placement bookkeeping is internal, not part of Rust's strict Ticket.
      return {binding:grant.binding,ticket:grant.ticket};
    } finally {clearTimeout(timeout);if(socketRequest===controller)socketRequest=null;}
  }
  function placementUnavailable() {
    if (!owned || !token || detachPending || disposed) return;
    placementSerial++;placementPending=true;firstTicket=null;
    for (const controller of contentRequests) controller.abort();
    socketRequest?.abort();socketRequest=null;
    emit({state:"admitted_awaiting_placement",avatars,grant:{binding:owned.binding,lot_id:owned.lot_id,lot_name:owned.lot_name,recovery_scope:owned.recovery_scope},message:"Your Sim is admitted and remains reserved, but the original lot could not place them in the world. Retry placement after the entry area is fixed, or request leave. No new Sim will be created."});
  }
  async function retryPlacement() {
    // Repeated retry clicks must not turn an in-flight reconnect into a
    // placement failure: that path aborts the original, still-valid request.
    if (!placementPending || !owned || active || socketRequest || disposed) return;
    const selection=owned, n=generation, placementEpoch=placementSerial;
    const sameRetryScope=()=>!disposed && generation===n && owned===selection && placementSerial===placementEpoch;
    try {
      const grant=await socketGrant();
      if(!sameRetryScope() || !token || detachPending)return;
      firstTicket=grant.ticket;placementPending=false;
      emit({state:"lot_ready",avatars,grant:{binding:owned.binding,lot_id:owned.lot_id,lot_name:owned.lot_name,recovery_scope:owned.recovery_scope},message:"Your admitted Sim is now placed in the world."});
    } catch(error) { if(sameRetryScope() && placementPending)placementUnavailable(); }
  }
  async function leave() {
    if(!token || !owned || disposed || active) return;
    const selection=owned, job=begin("leave");if(!job)return;
    detachPending=true;firstTicket=null;
    emit({state:"lot_leaving",avatars,message:"Leaving the property. Waiting for the server to save and detach your Sim…"});
    try {
      for(let attempt=0;attempt<40;attempt++) {
        const result=await request("leave",{avatar_id:selection.avatar_id,lot_id:selection.lot_id,operation:selection.operation},token,job.controller.signal);
        if(!current(job))return;
        if(result?.version!==1 || result.avatar_id!==selection.avatar_id) throw Error("Detach identity unconfirmed");
        if(result.state==="detached") {
          owned=null;pendingEntry=null;detachPending=false;placementPending=false;
          emit({state:"account_ready",avatars,message:"Your Sim has left the property."});
          return;
        }
        if(result.state!=="detach_pending" || !["leaving","blocked","detached"].includes(result.status)) throw Error("Detach unconfirmed");
        if(result.status==="blocked") {
          emit({state:"detach_pending",avatars,message:"Leaving is waiting for supported game behavior. Your Sim remains reserved and saved state has not been discarded. Retry leaving after the server resolves the blocker."});
          return;
        }
        await new Promise((resolve,reject)=>{
          const abort=()=>{clearTimeout(timer);reject(Error("Account operation cancelled"));};
          const timer=setTimeout(()=>{job.controller.signal.removeEventListener("abort",abort);resolve();},250);
          if(job.controller.signal.aborted)abort();else job.controller.signal.addEventListener("abort",abort,{once:true});
        });
      }
      throw Error("Detach still pending");
    }catch(error){if(current(job))emit({state:"detach_pending",avatars,message:"Leaving is not confirmed. Your Sim remains reserved until the server finishes saving and detaching. Retry leaving or sign out."});}
    finally{finish(job);}
  }
  async function logout() {
    if (disposed || active?.action === "logout") return;
    active?.controller.abort();
    generation++;
    active = null;
    const secret = token, hadOwned = !!(owned || pendingEntry || detachPending);
    socketRequest?.abort();socketRequest=null;
    token = null;
    avatars = [];
    destinations = []; admissionAvailable = false; owned = null; pendingEntry = null; firstTicket = null; detachPending = false; placementPending = false;
    creator=null;pendingCreation=null;creationUnknown=false;createdAvatarId=null;
    const job = begin("logout");
    if (!job) return;
    try {
      if (secret)
        await request("logout", { all: false }, secret, job.controller.signal);
      if (current(job))
        emit({ state: "signed_out", avatars: [], message: hadOwned ? "Signed out. Your Sim stays reserved until the property confirms saving and detach." : "Signed out." });
    } catch {
      if (current(job))
        emit({
          state: "signed_out",
          avatars: [],
          message:
            "This tab is signed out. Server sign-out was not confirmed; the session expires automatically.",
        });
    } finally {
      finish(job);
    }
  }
  function dispose() {
    disposed = true;
    generation++;
    for (const controller of contentRequests) controller.abort();
    active?.controller.abort();
    socketRequest?.abort();socketRequest=null;
    active = null;
    token = null;
    avatars = [];
    destinations = []; admissionAvailable = false; owned = null; pendingEntry = null; firstTicket = null; detachPending = false; placementPending = false;
    creator=null;pendingCreation=null;creationUnknown=false;createdAvatarId=null;
  }
  async function contentFiles() {
    if(creator&&!owned&&token&&!disposed&&!active){
      const selection=creator,n=generation,secret=token,controller=new AbortController();
      contentRequests.add(controller);const timer=setTimeout(()=>controller.abort(),60000);
      try{return await loadNativeContent({signal:controller.signal,expectedRelease:selection.release,
        current:()=>!disposed&&generation===n&&creator===selection&&token===secret,
        read:(path,signal)=>fetcher(new URL(path.replace('/native-content/','/native-content/creator/'),origin).href,{
          method:'POST',credentials:'omit',cache:'no-store',redirect:'error',signal,
          headers:{'Content-Type':'application/json',Authorization:'Bearer '+secret},body:JSON.stringify({release:selection.release})})});
      }finally{clearTimeout(timer);contentRequests.delete(controller);}
    }
    if (!token || !owned || detachPending || placementPending || disposed)
      throw Error("No active owned content admission.");
    const selection=owned,n=generation,secret=token,controller=new AbortController();
    contentRequests.add(controller);
    const timer=setTimeout(()=>controller.abort(),60000);
    const current=()=>!disposed&&generation===n&&owned===selection&&token===secret&&!detachPending&&!placementPending;
    try {
      return await loadNativeContent({signal:controller.signal,current,
        read:(path,signal)=>fetcher(new URL(path,origin).href,{
          method:'POST',credentials:'omit',cache:'no-store',redirect:'error',signal,
          headers:{'Content-Type':'application/json',Authorization:'Bearer '+secret},
          body:JSON.stringify({avatar_id:selection.avatar_id,lot_id:selection.lot_id,operation:selection.operation}),
        })});
    } finally {clearTimeout(timer);contentRequests.delete(controller);}
  }
  emit({
    state: "signed_out",
    avatars: [],
    message: "Sign in to your native Wonderland account.",
  });
  async function lotChat(action,payload,signal){
    if(!['read','send'].includes(action)||!token||!owned||placementPending||detachPending||disposed||active)
      throw Error('No active owned lot for chat.');
    if(!payload||typeof payload!=='object'||Array.isArray(payload)
      ||Object.keys(payload).sort().join(',')!==(action==='read'?'':'sequence,text'))throw Error('Invalid chat request.');
    const selection=owned,secret=token;
    const data={...payload,avatar_id:selection.avatar_id,lot_id:selection.lot_id,operation:selection.operation,
      lot_incarnation:selection.binding.lot_incarnation,action};
    const value=await request('chat',data,secret,signal);
    if(disposed||token!==secret||owned!==selection||detachPending||placementPending
      ||value.viewer_id!==selection.avatar_id||value.lot_id!==selection.lot_id
      ||value.lot_incarnation!==selection.binding.lot_incarnation)throw Error('Owned chat scope changed.');
    return value;
  }
  async function lotEod(query,signal){
    if(!token||!owned||placementPending||detachPending||disposed||active)throw Error('No active owned lot dialog.');
    const selection=owned,secret=token;
    const value=await request('eod',{avatar_id:selection.avatar_id,lot_id:selection.lot_id,operation:selection.operation,
      lot_incarnation:selection.binding.lot_incarnation,query},secret,signal);
    if(disposed||token!==secret||owned!==selection||detachPending||placementPending
      ||value.viewer_id!==selection.avatar_id||value.lot_id!==selection.lot_id
      ||value.lot_incarnation!==selection.binding.lot_incarnation)throw Error('Owned dialog scope changed.');
    return value;
  }
  return { login, refresh, logout, dispose, enter, socketGrant, leave, retryPlacement, placementUnavailable, contentFiles,openCreator,closeCreator,createAvatar,retryCreate,lotChat,lotEod };
}
