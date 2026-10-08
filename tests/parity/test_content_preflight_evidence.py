"""Evidence validation must not turn identical incomplete reports into readiness."""
import importlib.util,json,pathlib,unittest
ROOT=pathlib.Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('content_preflight',ROOT/'tools/replay/run_content_preflight.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
EXPECTED=(ROOT/'fixtures/reference/content-preflight.expected.json').read_bytes()
def encode(value):return (json.dumps(value,indent=2,sort_keys=True)+'\n').encode()
class Evidence(unittest.TestCase):
 def test_real_pinned_report_is_inspected_but_not_ready(self):
  doc=module.verify_report(EXPECTED,EXPECTED);self.assertTrue(doc['audit_completed']);self.assertFalse(doc['static_cohort_complete']);self.assertFalse(doc['whole_object_runtime_qualified'])
 def test_empty_and_truncated_reports_reject(self):
  for data in [b'',b'{}',b'[]',EXPECTED[:-1],EXPECTED[:300]]:
   with self.subTest(data=data[:20]),self.assertRaises(ValueError):module.verify_report(data,EXPECTED)
 def test_ready_flag_cannot_be_invented_in_both_sides(self):
  doc=json.loads(EXPECTED);doc['static_cohort_complete']=True;data=encode(doc)
  with self.assertRaises(ValueError):module.verify_report(data,data)
 def test_missing_declarations_cannot_be_dropped_even_when_outputs_match(self):
  doc=json.loads(EXPECTED);doc['resources'][0]['audit']['missing']=[];data=encode(doc)
  with self.assertRaises(ValueError):module.verify_report(data,data)
 def test_wrong_scope_is_not_interpreter_parity(self):
  doc=json.loads(EXPECTED);doc['scope']='original-engine-equivalence';data=encode(doc)
  with self.assertRaises(ValueError):module.verify_report(data,EXPECTED)
 def test_resource_removal_is_not_coverage(self):
  doc=json.loads(EXPECTED);doc['resources'].pop();data=encode(doc)
  with self.assertRaises(ValueError):module.verify_report(data,EXPECTED)
 def test_duplicate_json_key_cannot_hide_readiness_override(self):
  data=EXPECTED.replace(b'"schema": 1',b'"schema": 2, "schema": 1',1)
  with self.assertRaises(ValueError):module.verify_report(data,EXPECTED)
 def test_nonfinite_and_noninteger_counts_reject(self):
  for count in ['NaN','Infinity','4.0','1e9999','true']:
   data=EXPECTED.replace(b'"resource_count": 4',('"resource_count": '+count).encode(),1)
   with self.subTest(count=count),self.assertRaises(ValueError):module.verify_report(data,EXPECTED)
 def test_newline_and_encoding_changes_cannot_masquerade_as_identical(self):
  for data in [EXPECTED+b'\n',b'\xef\xbb\xbf'+EXPECTED,EXPECTED.replace(b'\n',b'\r\n')]:
   with self.assertRaises(ValueError):module.verify_report(data,EXPECTED)
 def test_oversize_report_rejects_before_parsing(self):
  with self.assertRaises(ValueError):module.verify_report(b' '*(1024*1024+1),EXPECTED)
if __name__=='__main__':unittest.main()
