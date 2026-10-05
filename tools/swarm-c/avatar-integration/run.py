#!/usr/bin/env python3
"""Generate an isolated temporary probe manifest with pinned, read-only A.
SWARM_A_PATH identifies A's published checkout; no A path dependency is shipped.
"""
import pathlib, subprocess, os, sys, json
PIN='8a0e251d19e222a0a6833d7408ca629f674e1729'
root=pathlib.Path(__file__).resolve().parents[3]
a=pathlib.Path(os.environ.get('SWARM_A_PATH',str(root.parent/'wonderland-swarm-a'))).resolve()
head=subprocess.check_output(['git','-C',str(a),'rev-parse','HEAD'],text=True).strip()
if head!=PIN:sys.exit('A checkout must equal '+PIN+'; observed '+head)
out=pathlib.Path(sys.argv[1]).resolve();out.mkdir(parents=True,exist_ok=True)
fixture=(a/'crates/sim-core/tests/runtime_avatar_integration.rs').read_text().split('#[test]')[0]
fixture+='''
pub fn presentation_runtime()->SimRuntime {
 let (content,mut state,entity)=state();
 let avatar=state.entities.get_mut(&entity.object_id).unwrap().avatar.as_mut().unwrap();
 let mut first=AnimationState::new(metadata("owner-adult"),false).unwrap();first.looping=true;
 let mut second=AnimationState::new(metadata("global"),true).unwrap();second.looping=true;second.weight=3.0;second.speed=2.0;
 avatar.animations.animations=vec![first,second];
 avatar.animations.carry=Some(AnimationState::new(metadata("a2o-rarm-carry-loop"),false).unwrap());
 SimRuntime::from_state(state,content,RuntimeRole::Authority).unwrap()
}
'''
(out/'a_fixture.rs').write_text(fixture)
manifest='''[package]
name="wonderland-c-avatar-a-boundary-probe"
version="0.1.0"
edition="2021"
rust-version="1.75"
license="MPL-2.0"
[workspace]
[dependencies]
sim-core={path=A_PATH}
wonderland-avatar-view={path=C_PATH}
wonderland-render-core={path=CORE_PATH}
[profile.release]
overflow-checks=true
'''.replace('A_PATH',json.dumps(str(a/'crates/sim-core'))).replace('C_PATH',json.dumps(str(root/'crates/avatar-view'))).replace('CORE_PATH',json.dumps(str(root/'crates/render-core')))
(out/'Cargo.toml').write_text(manifest);(out/'src').mkdir(exist_ok=True);(out/'src/lib.rs').write_text((pathlib.Path(__file__).parent/'probe.rs').read_text())
env=dict(os.environ);env['A_FIXTURE_PATH']=str(out/'a_fixture.rs')
subprocess.run(['cargo','test','--offline','--manifest-path',str(out/'Cargo.toml'),'--','--nocapture'],check=True,env=env)
