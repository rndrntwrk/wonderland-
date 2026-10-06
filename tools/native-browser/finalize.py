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
print('Strict native browser corrections assembled')
