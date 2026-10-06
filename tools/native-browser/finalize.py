#!/usr/bin/env python3
"""Apply reviewed final candidate edits before canonical source publication."""
import runpy
replace=runpy.run_path('tools/native-browser/assemble.py')['replace']
p='apps/web-shell/src/native_lot.rs'
replace(p,'v.show_roofs=!v.show_roofs','v.show_roofs = !v.show_roofs')
replace(p,'*v=!*v','*v = !*v')
replace(p,'if let Some(request)=request {\n                        if let Ok(bytes)=encode_checkpoint_request(request){if !self.host("control",Some(&bytes)){self.fail("The recovery request could not be sent.");}}\n                    }',
        'let sent=request.and_then(|request|encode_checkpoint_request(request).ok()).is_some_and(|bytes|self.host("control",Some(&bytes)));\n                    if !sent{self.fail("The recovery request could not be sent.");}')
replace(p,'if let Some(p)=r.player.as_mut(){if p.dismiss_unknown().is_ok(){ctl.status.set(p.status());}}',
        'if let Some(p)=r.player.as_mut() && p.dismiss_unknown().is_ok(){ctl.status.set(p.status());}')
replace(p,'        self.live.set(false);self.choices.set(Vec::new());self.notice.set("Connecting to this property’s native runtime…".into());',
        '        if !self.current(generation){return;}\n        self.live.set(false);self.choices.set(Vec::new());self.notice.set("Connecting to this property’s native runtime…".into());')
replace(p,'if ready{self.live.try_set(true);self.notice.try_set(String::new());self.host("ready",None);self.wake.try_update(|n|*n=n.saturating_add(1));}',
        'if ready{let was_live=self.live.get_untracked();self.live.try_set(true);if !was_live{self.notice.try_set(String::new());self.host("ready",None);}self.wake.try_update(|n|*n=n.saturating_add(1));}')
p='crates/game-runtime/examples/native_browser_peer.rs'
replace(p,'use std::io::{self, BufRead, Write};','use std::io::{self, BufRead, Read, Write};')
replace(p,'LotModel::new(8,8,1)?','LotModel::new(8,8,1).map_err(|_|"Invalid controlled lot geometry")?')
replace(p,'input.as_bytes().chunks_exact(2)','input.as_bytes().as_chunks::<2>().0.iter()')
print('Strict native browser corrections assembled')
