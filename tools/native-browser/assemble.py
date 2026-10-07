#!/usr/bin/env python3
"""One-time guarded integration assembly for the hosted editing environment.
Canonical results are published before handoff. Original sources are untouched.
"""
from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text()
    if new in text:
        return
    if text.count(old) != 1:
        raise SystemExit(f'Expected exactly one integration anchor: {path}')
    target.write_text(text.replace(old, new))

replace('crates/game-runtime/src/live_wire.rs', 'mod guard;', 'mod guard;\npub mod player;')
p='crates/game-runtime/src/live_wire/player.rs'
replace(p,'    sequence: u64,','    sequence: u64,\n    pending_after: u64,')
replace(p,'sequence:0,pending:None','sequence:0,pending_after:0,pending:None')
replace(p,'self.wire.replica().projection().map_err(|_|"Native world is not live")?;',
        'if self.wire.replica().status()!=crate::live_session::SessionStatus::Live{return Err("Native world is not live");}')
replace(p,'|tick|self.wire.replica().cursor().is_none_or(|cursor|cursor.completed_tick<tick)',
        '|tick|tick<=self.pending_after || self.wire.replica().cursor().is_none_or(|cursor|cursor.completed_tick<tick)')
# Both preparation paths reserve the same sequence/receipt boundary, not state.
target=Path(p);text=target.read_text();old='self.sequence=sequence;self.pending=Some(bytes.clone());'
new='self.pending_after=self.wire.replica().cursor().ok_or("Native cursor is unavailable")?.completed_tick;self.sequence=sequence;self.pending=Some(bytes.clone());'
if new not in text:
    if text.count(old)!=2:raise SystemExit('Expected two intent preparation paths')
    target.write_text(text.replace(old,new))

if Path('apps/web-shell/src/native_lot.rs').exists():
    replace('apps/web-shell/Cargo.toml', 'wonderland-game-services.workspace = true',
            'wonderland-game-services.workspace = true\nwonderland-game-runtime.workspace = true')
    replace('apps/web-shell/src/lib.rs', 'pub mod startup;', 'pub mod startup;\n#[cfg(target_arch = "wasm32")]\npub mod native_lot;')
    replace('apps/web-shell/src/startup.rs', '    pub gateway_url: Option<String>,',
            '    pub gateway_url: Option<String>,\n    #[serde(default)]\n    pub native_lots: bool,')
    replace('apps/web-shell/src/startup.rs', '    Ok(config)\n}',
            '    if config.native_lots && config.mode != ClientMode::Connected { return Err("Native lots require connected mode.".into()); }\n    Ok(config)\n}')
    replace('apps/web-shell/src/startup_view.rs', 'gateway_url=config.gateway_url.unwrap_or_default()/>',
            'gateway_url=config.gateway_url.unwrap_or_default() native_lots=config.native_lots/>')
    replace('apps/web-shell/src/connected.rs', 'pub fn ConnectedGame(#[prop(into)] gateway_url: String) -> impl IntoView {',
            'pub fn ConnectedGame(#[prop(into)] gateway_url: String, #[prop(default=false)] native_lots: bool) -> impl IntoView {')
    replace('apps/web-shell/src/connected.rs', '3=>view!{<crate::connected_world::ConnectedLotView/>}.into_any()',
            '3=>if native_lots {view!{<crate::native_lot::NativeLot/>}.into_any()} else {view!{<crate::connected_world::ConnectedLotView/>}.into_any()}')
    replace('apps/web-shell/index.html','    <link data-trunk rel="copy-file" href="public/wonderland-config.json" />',
            '    <link data-trunk rel="copy-file" href="public/wonderland-config.json" />\n    <link data-trunk rel="copy-file" href="public/native-socket.mjs" />')
    addition = Path('tools/native-browser/admission-method.rs.txt').read_text()
    target = Path('apps/web-shell/src/connected_bridge.rs');text = target.read_text()
    if 'pub async fn admit_native_lot' not in text:target.write_text(text + '\n' + addition)
    p='apps/web-shell/src/native_lot.rs'
    replace(p,'r.generation=r.generation.checked_add(1).unwrap_or(u64::MAX);',
            'let Some(next)=r.generation.checked_add(1) else {r.close();return (u64::MAX,false);};r.generation=next;')
    replace(p,'if r.scope!=new_scope{r.player=None;r.last_request=0;}',
            'if r.scope!=new_scope{r.player=None;r.last_request=0;self.world.set(None);self.projection.set(None);self.status.set(ActionStatus::Idle);}')
    replace(p,'let resume=r.player.is_some();\n            if let Some(player)=r.player.as_mut(){player.disconnect();if player.reconnect().is_err(){r.player=None;}}',
            'if let Some(player)=r.player.as_mut(){player.disconnect();if player.reconnect().is_err(){r.player=None;}}\n            let resume=r.player.is_some();')
    replace(p,'p.queues.iter().flat_map(|q|q.entries.clone())',
            'p.queues.iter().filter(|q|Some(q.actor)==ctl.resources.with_value(|r|r.player.as_ref().map(|p|p.actor()))).flat_map(|q|q.entries.clone())')
    replace(p,'let Some((generation,resume))=self.resources.try_update_value',
            'let Some((generation,resume))=self.resources.try_update_value')
# Explicit resume suppresses a second bootstrap on a retained runtime.
replace('apps/web-shell/public/native-socket.mjs',"JSON.stringify({type: 'native_auth', ticket})",
        "JSON.stringify(options.resume === true ? {type: 'native_auth', ticket, resume: true} : {type: 'native_auth', ticket})")
print('Guarded integration assembly complete')
