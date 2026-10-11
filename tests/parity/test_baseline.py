"""F's drift gate tests; fixtures are explicit small declaration sets, not a VM oracle."""
from __future__ import annotations
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('f_baseline', ROOT / 'tools/replay/baseline.py')
b = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(b)

SOLUTION = ('Microsoft Visual Studio Solution File, Format Version 12.00\n'
 'Project("{FAE04EC0-301F-11D3-BF4B-00C04F79EFBC}") = "Game", "game\\Game.csproj", "{11111111-1111-1111-1111-111111111111}"\nEndProject\n')
PROJECT = '<Project xmlns="http://schemas.microsoft.com/developer/msbuild/2003"><ItemGroup><Compile Include="One.cs"/></ItemGroup></Project>'

class SourceFixture:
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.put('TSOClient/FreeSO.sln', SOLUTION)
        self.put('TSOClient/game/Game.csproj', PROJECT)
        self.put('TSOClient/game/One.cs', 'class One {}\n')
        self.put('.gitmodules', '')

    def put(self, name, value):
        p = self.root / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(value if isinstance(value, bytes) else value.encode())
        return p

    def census(self): return b.collect_source(self.root)
    def row(self): return self.census()['projects'][0]
    def snapshot(self): return b.freeze_source(self.census())

class BaselineTests(SourceFixture, unittest.TestCase):
    def test_deterministic_census_has_no_absolute_paths_or_clock(self):
        a = self.census()
        self.assertEqual(a, self.census())
        self.assertNotIn(str(self.root), json.dumps(a))
        self.assertEqual(a['scope'], 'declared-solution-not-msbuild-evaluated')
        self.assertEqual(a['projects'][0]['compile'][0]['target'], 'TSOClient/game/One.cs')

    def test_source_byte_change_changes_the_compile_cohort_digest(self):
        old = self.snapshot()
        self.put('TSOClient/game/One.cs', 'class One { int changed; }')
        self.assertNotEqual(old, self.snapshot())

    def test_commented_compile_is_not_active_declaration(self):
        self.put('TSOClient/game/Game.csproj', '<Project><!-- <Compile Include="Absent.cs"/> --><ItemGroup><Compile Include="One.cs"/></ItemGroup></Project>')
        self.assertEqual(len(self.row()['compile']), 1)

    def test_parent_and_item_conditions_survive_without_evaluation(self):
        self.put('TSOClient/game/Game.csproj', '<Project><ItemGroup Condition="Debug"><Compile Include="One.cs" Condition="Windows"/></ItemGroup></Project>')
        self.assertEqual(self.row()['compile'][0]['conditions'], ['Debug', 'Windows'])

    def test_link_is_metadata_not_the_physical_read_path(self):
        self.put('TSOClient/game/Game.csproj', '<Project><ItemGroup><Compile Include="..\\shared.cs"><Link>Shown\\One.cs</Link></Compile></ItemGroup></Project>')
        self.put('TSOClient/shared.cs', 'class Shared {}')
        r = self.row()['compile'][0]
        self.assertEqual(r['target'], 'TSOClient/shared.cs')
        self.assertEqual(r['link'], 'Shown\\One.cs')
        self.assertEqual(r['state'], 'present')

    def test_missing_compile_is_retained_and_not_called_covered(self):
        (self.root/'TSOClient/game/One.cs').unlink()
        self.assertEqual(self.row()['compile'][0]['state'], 'missing')
        self.assertEqual(self.census()['counts']['missing_compile'], 1)

    def test_duplicate_compile_declarations_are_retained(self):
        self.put('TSOClient/game/Game.csproj', '<Project><ItemGroup><Compile Include="One.cs"/><Compile Include="One.cs"/></ItemGroup></Project>')
        self.assertEqual(len(self.row()['compile']), 2)

    def test_dynamic_wildcard_and_remove_are_explicitly_unresolved(self):
        self.put('TSOClient/game/Game.csproj', '<Project><ItemGroup><Compile Include="$(Root)\\X.cs"/><Compile Include="**\\*.cs"/><Compile Remove="One.cs"/></ItemGroup></Project>')
        self.assertEqual([v['state'] for v in self.row()['compile']], ['unevaluated', 'unevaluated', 'unevaluated'])

    def test_sdk_implicit_compile_does_not_become_an_empty_complete_cohort(self):
        self.put('TSOClient/game/Game.csproj', '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>')
        self.assertEqual(self.row()['implicit_compile'], 'unevaluated-sdk-defaults')

    def test_import_and_project_reference_are_recorded_without_execution(self):
        self.put('TSOClient/game/Game.csproj', '<Project><Import Project="$(MSBuildToolsPath)\\Microsoft.CSharp.targets"/><ItemGroup><ProjectReference Include="..\\Other.csproj"/></ItemGroup></Project>')
        row = self.row()
        self.assertEqual(len(row['imports']), 1)
        self.assertEqual(row['references'][0]['target'], 'TSOClient/Other.csproj')
        self.assertEqual(row['references'][0]['state'], 'missing')

    def test_xml_external_entities_are_rejected(self):
        self.put('TSOClient/game/Game.csproj', '<!DOCTYPE Project [<!ENTITY secret SYSTEM "file:///etc/passwd">]><Project>&secret;</Project>')
        with self.assertRaises(b.BaselineError): self.census()

    def test_utf8_bom_in_solution_and_project_is_supported(self):
        self.put('TSOClient/FreeSO.sln', b'\xef\xbb\xbf'+SOLUTION.encode())
        self.put('TSOClient/game/Game.csproj', b'\xef\xbb\xbf'+PROJECT.encode())
        self.assertEqual(self.census()['counts']['present_projects'], 1)

    def test_missing_project_is_not_dropped(self):
        (self.root/'TSOClient/game/Game.csproj').unlink()
        self.assertEqual(self.row()['state'], 'missing')

    def test_solution_folder_is_not_a_missing_csharp_project(self):
        self.put('TSOClient/FreeSO.sln', SOLUTION+'Project("{2150E333-8FDC-42A3-9474-1A3956D46DE8}") = "Folder", "Folder", "{22222222-2222-2222-2222-222222222222}"\nEndProject\n')
        self.assertEqual(len(self.census()['projects']), 1)
        self.assertEqual(len(self.census()['solution_folders']), 1)

    def test_duplicate_solution_identity_is_rejected(self):
        self.put('TSOClient/FreeSO.sln', SOLUTION+SOLUTION.split('\n', 1)[1])
        with self.assertRaises(b.BaselineError): self.census()

    def test_unparseable_project_line_fails_instead_of_silently_omitting(self):
        self.put('TSOClient/FreeSO.sln', SOLUTION+'Project(broken) = unrecognised\n')
        with self.assertRaises(b.BaselineError): self.census()

    def test_parent_traversal_cannot_read_outside_checkout(self):
        self.put('TSOClient/game/Game.csproj', '<Project><Compile Include="../../../outside.cs"/></Project>')
        with self.assertRaises(b.BaselineError): self.census()

    def test_absolute_and_network_include_paths_are_rejected(self):
        for path in ['C:\\private.cs', '/etc/passwd', '\\\\host\\secret.cs']:
            with self.subTest(path=path):
                self.put('TSOClient/game/Game.csproj', '<Project><Compile Include="'+path+'"/></Project>')
                with self.assertRaises(b.BaselineError): self.census()

    def test_symlink_source_and_parent_are_rejected(self):
        source=self.root/'TSOClient/game/One.cs';source.unlink()
        source.symlink_to(self.root/'missing')
        with self.assertRaises(b.BaselineError): self.census()

    def test_size_limit_rejects_before_unbounded_read(self):
        self.put('TSOClient/game/One.cs', b'x'*(b.MAX_FILE_BYTES+1))
        with self.assertRaises(b.BaselineError): self.census()

    def test_submodule_project_is_not_confused_with_vendored_missing_source(self):
        self.put('.gitmodules', '[submodule "engine"]\n path = Other/engine\n url = https://example.invalid/engine.git\n')
        self.put('TSOClient/FreeSO.sln', SOLUTION.replace('game\\Game.csproj', '..\\Other\\engine\\Game.csproj'))
        self.assertEqual(self.row()['state'], 'submodule-not-inspected')
        self.assertEqual(self.census()['submodules'][0]['path'], 'Other/engine')

    def test_first_difference_localizes_the_changed_field(self):
        self.assertEqual(b.first_difference({'a':[{'hash':'old'}]}, {'a':[{'hash':'new'}]}), '$.a[0].hash')

    def test_strict_json_rejects_duplicate_keys_and_nan(self):
        for text in ['{"version":1,"version":2}', '{"value":NaN}', '{"value":Infinity}']:
            p=self.put('bad.json', text)
            with self.subTest(text=text), self.assertRaises(b.BaselineError): b.read_json(self.root,'bad.json')

    def test_json_exponent_overflow_cannot_hide_nonfinite_numbers(self):
        self.put('bad.json', '{"value":1e9999}')
        with self.assertRaises(b.BaselineError): b.read_json(self.root, 'bad.json')

    def test_malformed_solution_guid_is_not_an_admitted_identity(self):
        self.put('TSOClient/FreeSO.sln', SOLUTION.replace('11111111-1111-1111-1111-111111111111', '1' * 36))
        with self.assertRaises(b.BaselineError): self.census()

    def test_symlink_directory_cannot_supply_a_source_input(self):
        project = self.root / 'TSOClient/game'
        project.rename(self.root / 'moved')
        project.symlink_to(self.root / 'moved', target_is_directory=True)
        with self.assertRaises(b.BaselineError): self.census()

    def test_boundary_source_drift_is_not_approved_by_reusing_an_old_evidence_label(self):
        path='crates/contracts/src/lib.rs';self.put(path,'//! preview not a multiplayer wire protocol\n')
        spec={'id':'preview','realm':'presentation','sources':[{'path':path,'sha256':b.sha256((self.root/path).read_bytes())}],
              'consumers':[], 'evidence':[], 'status':'observed-not-frozen', 'owner':'F', 'notes':'No authority.'}
        self.assertEqual(b.check_boundary(self.root,spec), [])
        self.put(path,'// changed protocol\n')
        self.assertTrue(b.check_boundary(self.root,spec))

    def test_presentation_and_legacy_sources_cannot_be_relabelled_native(self):
        for path in ['crates/contracts/src/lib.rs','crates/vm-protocol/src/snapshot.rs']:
            self.put(path,'text')
            spec={'id':'x','realm':'native','sources':[{'path':path,'sha256':b.sha256(b'text')}], 'consumers':[], 'evidence':[], 'status':'observed-not-frozen','owner':'F','notes':'x'}
            with self.assertRaises(b.BaselineError):b.check_boundary(self.root,spec)

    def test_boundary_cannot_claim_complete_acceptance_from_source_presence(self):
        self.put('crates/game-runtime/src/a.rs','test')
        spec={'id':'x','realm':'native','sources':[{'path':'crates/game-runtime/src/a.rs','sha256':b.sha256(b'test')}], 'consumers':[], 'evidence':[], 'status':'complete','owner':'F','notes':'x'}
        with self.assertRaises(b.BaselineError):b.check_boundary(self.root,spec)

    def test_boundary_rejects_empty_pins_and_duplicate_sources(self):
        p='crates/game-runtime/src/a.rs';self.put(p,'x')
        pin={'path':p,'sha256':b.sha256(b'x')}
        for pins in [[],[pin,pin]]:
            spec={'id':'x','realm':'native','sources':pins,'consumers':[], 'evidence':[], 'status':'observed-not-frozen','owner':'F','notes':'x'}
            with self.subTest(pins=pins), self.assertRaises(b.BaselineError):b.check_boundary(self.root,spec)

    def test_boundary_missing_evidence_path_is_not_a_pass(self):
        p='crates/game-runtime/src/a.rs';self.put(p,'x')
        spec={'id':'x','realm':'native','sources':[{'path':p,'sha256':b.sha256(b'x')}], 'consumers':[], 'evidence':[{'path':'tests/missing.rs','kind':'fixture-test-not-executed'}], 'status':'observed-not-frozen','owner':'F','notes':'x'}
        self.assertTrue(b.check_boundary(self.root,spec))

    def test_inspected_source_evidence_cannot_be_promoted_to_original_engine_parity(self):
        p='crates/game-runtime/src/a.rs';self.put(p,'x')
        spec={'id':'x','realm':'native','sources':[{'path':p,'sha256':b.sha256(b'x')}], 'consumers':[], 'evidence':[{'path':p,'kind':'original-engine-equivalent'}], 'status':'observed-not-frozen','owner':'F','notes':'x'}
        with self.assertRaises(b.BaselineError):b.check_boundary(self.root,spec)


class GateFixture(SourceFixture):
    # Only run these new tests; inherited source cases remain in BaselineTests.
    def setUp(self):
        super().setUp()
        sources={
          'ui-preview':'crates/contracts/src/lib.rs',
          'native-identity':'crates/sim-core/src/ids.rs',
          'native-accepted-tick':'crates/sim-core/src/runtime.rs',
          'native-checkpoint':'crates/sim-core/src/snapshot.rs',
          'native-effects':'crates/sim-core/src/effects.rs',
          'native-replay':'crates/game-runtime/src/live_session.rs',
          'native-wire':'crates/game-runtime/src/live_wire.rs',
          'native-player-admission':'crates/game-runtime/src/live_wire/player.rs',
          'native-visual-projection':'crates/game-runtime/src/avatar_projection.rs',
          'legacy-vmnet':'crates/vm-protocol/src/lib.rs',
          'legacy-gateway':'services/browser-gateway/src/server.rs',
          'content-identity':'crates/content-ir/src/manifest.rs',
        }
        specs=[]
        for identity,path in sorted(sources.items()):
            self.put(path,'// authored contract fixture\n')
            specs.append({'id':identity,'realm':b.source_realm(path),'owner':'F','status':'observed-not-frozen',
                          'sources':[{'path':path,'sha256':b.sha256((self.root/path).read_bytes())}],
                          'consumers':[],'evidence':[],'notes':'Test fixture, not game-code execution.'})
        self.baseline={'schema':1,'source_commit':b.SOURCE_COMMIT,'inspected_commit':'1'*40,
                       'source':self.snapshot(),'gitlinks':{},'dispositions':{'TSOClient/game/Game.csproj':
                       {'owner':'A','destination':'Declared native simulation lane','acceptance':'not-assessed'}},
                       'limits':'Declared sources only, no original equivalence.'}
        self.boundaries={'schema':1,'inspected_commit':'1'*40,'boundaries':specs,'open_decisions':['Reference runtime not qualified.']}
        self.save()

    def save(self):
        self.put(b.SOURCE_MANIFEST,b.encoded(self.baseline))
        self.put(b.BOUNDARY_MANIFEST,b.encoded(self.boundaries))

class GateTests(GateFixture, unittest.TestCase):
    def test_complete_intake_map_can_pass_without_claiming_execution(self):
        report,_=b.audit(self.root)
        self.assertTrue(report['passed'])
        self.assertEqual(report['original_equivalence'],'not-tested')
        self.assertEqual(report['release_qualification'],'not-tested')

    def test_deleted_boundary_and_duplicate_boundary_are_rejected(self):
        self.boundaries['boundaries'].pop();self.save()
        with self.assertRaises(b.BaselineError):b.audit(self.root)
        self.boundaries['boundaries'].append(copy.deepcopy(self.boundaries['boundaries'][0]));self.save()
        with self.assertRaises(b.BaselineError):b.audit(self.root)

    def test_deleted_project_disposition_cannot_leave_green_coverage(self):
        self.baseline['dispositions'].clear();self.save()
        report,_=b.audit(self.root)
        self.assertFalse(report['passed'])
        self.assertIn('TSOClient/game/Game.csproj',' '.join(report['issues']))

    def test_mixed_inspection_revisions_fail(self):
        self.boundaries['inspected_commit']='2'*40;self.save()
        with self.assertRaises(b.BaselineError):b.audit(self.root)

    def test_unknown_schema_and_boolean_schema_fail(self):
        for value in [2,True,'1']:
            with self.subTest(value=value):
                self.baseline['schema']=value;self.save()
                with self.assertRaises(b.BaselineError):b.audit(self.root)

    def test_unknown_fields_cannot_smuggle_an_evidence_override(self):
        self.boundaries['approved']=True;self.save()
        with self.assertRaises(b.BaselineError):b.audit(self.root)

    def test_source_delta_is_localized_to_its_project(self):
        self.put('TSOClient/game/One.cs','class Changed {}')
        report,_=b.audit(self.root)
        self.assertFalse(report['passed'])
        self.assertIn('TSOClient/game/Game.csproj',' '.join(report['issues']))

    def test_required_boundary_cannot_be_reassigned_to_a_different_realm(self):
        spec=next(v for v in self.boundaries['boundaries'] if v['id']=='legacy-vmnet')
        spec['realm']='native';spec['sources']=[{'path':'crates/sim-core/src/runtime.rs','sha256':b.sha256((self.root/'crates/sim-core/src/runtime.rs').read_bytes())}]
        self.save()
        with self.assertRaises(b.BaselineError):b.audit(self.root)

    def test_cli_success_and_failure_status_are_real_process_exits(self):
        import subprocess,sys
        cmd=[sys.executable,'-B',str(ROOT/'tools/replay/baseline.py'),'--root',str(self.root)]
        green=subprocess.run(cmd,capture_output=True,text=True,timeout=5)
        self.assertEqual(green.returncode,0,green.stdout+green.stderr)
        self.assertTrue(json.loads(green.stdout)['passed'])
        self.put('crates/sim-core/src/runtime.rs','changed')
        red=subprocess.run(cmd,capture_output=True,text=True,timeout=5)
        self.assertEqual(red.returncode,1,red.stdout+red.stderr)
        self.assertFalse(json.loads(red.stdout)['passed'])

    def test_existing_output_is_not_overwritten_and_no_manifest_is_rewritten(self):
        import subprocess,sys
        out=self.root/'evidence';out.mkdir();self.put('evidence/sentinel','retain me')
        before=(self.root/b.SOURCE_MANIFEST).read_bytes()
        result=subprocess.run([sys.executable,'-B',str(ROOT/'tools/replay/baseline.py'),'--root',str(self.root),'--output',str(out)],capture_output=True,text=True,timeout=5)
        self.assertNotEqual(result.returncode,0)
        self.assertEqual((out/'sentinel').read_text(),'retain me')
        self.assertEqual(before,(self.root/b.SOURCE_MANIFEST).read_bytes())

    def test_report_binds_the_complete_census_bytes(self):
        import subprocess,sys
        out=self.root/'new-evidence'
        result=subprocess.run([sys.executable,'-B',str(ROOT/'tools/replay/baseline.py'),'--root',str(self.root),'--output',str(out)],capture_output=True,text=True,timeout=5)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        report=json.loads((out/'report.json').read_text())
        self.assertEqual(report['census_artifact_sha256'],b.sha256((out/'source-census.json').read_bytes()))
        self.assertIsNone(report['tested_checkout'])
        self.assertEqual(report['execution_scope'],'unattested-input-copy')

    def test_empty_source_solutions_and_unevaluated_conditions_are_not_silently_accepted(self):
        self.put('TSOClient/FreeSO.sln','no project data')
        with self.assertRaises(b.BaselineError):b.audit(self.root)


class ProvenanceTests(GateFixture, unittest.TestCase):
    def git(self, *args):
        import subprocess, os
        env = dict(os.environ, GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull)
        return subprocess.check_output(['git', '-C', str(self.root), '-c', 'user.name=Baseline Tests',
             '-c', 'user.email=baseline@example.invalid', '-c', 'commit.gpgsign=false', *args],
             stderr=subprocess.PIPE, text=True, timeout=10, env=env).strip()

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'Bounded evidence fixture')

    def initialize(self):
        self.put('tools/replay/baseline.py', (ROOT/'tools/replay/baseline.py').read_bytes())
        self.git('init', '-q'); self.commit()

    def test_identified_source_inputs_match_actual_git_blobs(self):
        self.initialize()
        identity=b.git_identity(self.root)
        self.assertEqual(identity['commit'],self.git('rev-parse','HEAD'))
        result=b.verify_git_inputs(self.root,self.census())
        self.assertGreater(result['regular_inputs_matched'],12)
        self.assertEqual(result['gitlinks'],{})

    def test_dirty_checkout_cannot_produce_clean_revision_evidence(self):
        self.initialize();self.put('TSOClient/game/One.cs','modified')
        with self.assertRaises(b.BaselineError):b.git_identity(self.root)

    def test_untracked_gate_files_are_not_part_of_the_claimed_commit(self):
        self.initialize();self.put('new-untracked.py','test')
        with self.assertRaises(b.BaselineError):b.git_identity(self.root)

    def test_ignored_file_cannot_masquerade_as_a_committed_consumer(self):
        self.put('.gitignore','ignored-consumer.rs\n')
        self.put('ignored-consumer.rs','not tracked')
        self.boundaries['boundaries'][0]['consumers'].append('ignored-consumer.rs');self.save()
        self.initialize()
        self.assertTrue(b.audit(self.root)[0]['passed'])
        b.git_identity(self.root)
        with self.assertRaisesRegex(b.BaselineError,'not a tracked regular file'):
            b.verify_git_inputs(self.root,self.census())

    def test_skip_worktree_cannot_hide_different_executed_source_bytes(self):
        self.initialize();self.git('update-index','--skip-worktree','TSOClient/game/One.cs')
        self.put('TSOClient/game/One.cs','class UncommittedChange {}')
        self.baseline['source']=self.snapshot();self.save();self.commit()
        b.git_identity(self.root)
        self.assertTrue(b.audit(self.root)[0]['passed'])
        with self.assertRaisesRegex(b.BaselineError,'differs from committed bytes'):
            b.verify_git_inputs(self.root,self.census())

    def test_gitlink_change_is_not_hidden_by_unchanged_gitmodules(self):
        self.put('.gitmodules','[submodule "engine"]\n path = Other/engine\n url = https://example.invalid/engine.git\n')
        self.baseline['source']=self.snapshot();self.baseline['gitlinks']={'Other/engine':None};self.save()
        self.initialize()
        self.git('update-index','--add','--cacheinfo','160000,'+'a'*40+',Other/engine')
        self.git('commit','-qm','Changed submodule pointer')
        with self.assertRaisesRegex(b.BaselineError,'gitlink changed'):
            b.verify_git_inputs(self.root,self.census())

    def test_matching_gitlink_is_recorded_without_initializing_payload(self):
        self.put('.gitmodules','[submodule "engine"]\n path = Other/engine\n url = https://example.invalid/engine.git\n')
        self.baseline['source']=self.snapshot();self.baseline['gitlinks']={'Other/engine':'a'*40};self.save()
        self.initialize()
        self.git('update-index','--add','--cacheinfo','160000,'+'a'*40+',Other/engine')
        self.git('commit','-qm','Declared submodule pointer')
        self.assertEqual(b.verify_git_inputs(self.root,self.census())['gitlinks'],{'Other/engine':'a'*40})
        self.assertFalse((self.root/'Other/engine').exists())

    def test_require_git_cli_reports_the_actual_execution_revision(self):
        import subprocess, sys
        self.initialize()
        result=subprocess.run([sys.executable,'-B',str(self.root/'tools/replay/baseline.py'),'--require-git',
                  '--root',str(self.root)],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertEqual(json.loads(result.stdout)['tested_checkout']['commit'],self.git('rev-parse','HEAD'))


if __name__ == '__main__': unittest.main()
