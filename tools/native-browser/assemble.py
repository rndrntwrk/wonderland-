#!/usr/bin/env python3
"""One-time guarded integration assembly for the hosted editing environment.
Idempotent once the assembled sources are published; never touches original data.
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
# Browser call sites are assembled only when the native view source is present.
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
    addition = Path('tools/native-browser/admission-method.rs.txt').read_text()
    target = Path('apps/web-shell/src/connected_bridge.rs')
    text = target.read_text()
    if 'pub async fn admit_native_lot' not in text:
        target.write_text(text + '\n' + addition)
print('Guarded integration assembly complete')
