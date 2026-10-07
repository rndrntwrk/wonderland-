"""Authored validator fixtures, not substitutes for the original placement engine."""
from pathlib import Path
import copy
import importlib.util
import subprocess
import sys
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('placement_compare', ROOT/'tools/replay/compare_placement.py')
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
CASES = (ROOT/'fixtures/reference/placement-cases.tsv').read_bytes()
HEADER = 'case\tts1\tfacing\tquery_status\tquery_blocker\tcommit_status\tcommit_blocker\tquery_unchanged\tafter_x\tafter_y\tafter_level\tafter_facing\tblocker_x\tblocker_y\tblocker_level\tblocker_facing\tentities\trecover_status\trecover_x\trecover_y\tretry_status\tretry_blocker\tfinal_x\tfinal_y\tfinal_facing'

def authored_trace():
    lines = [HEADER]
    for line in CASES.decode().splitlines()[1:]:
        name, *nums = line.split('\t')
        x,y,flags,wall,heights,allow,status=map(int, nums)
        for mode in (1,0):
            for facing in (0,2,4,6):
                blocker = 2 if status==10 else 0
                after=(x,y,1,facing) if status==-1 else (40,40,1,0)
                values=[name,mode,facing,status,blocker,status,blocker,1,*after,88,56,1,0,2,-1,40,72,10,2,40,72,0]
                lines.append('\t'.join(map(str,values)))
    return ('\n'.join(lines)+'\n').encode()

def alter(data, name, field, value):
    rows=data.decode().splitlines(); idx=HEADER.split('\t').index(field)
    for n,row in enumerate(rows[1:],1):
        cells=row.split('\t')
        if cells[:3]==[name,'1','0']:
            cells[idx]=str(value);rows[n]='\t'.join(cells);break
    return ('\n'.join(rows)+'\n').encode()

class PlacementComparatorTests(unittest.TestCase):
    def test_complete_equal_trace_matches_without_claiming_full_vm_or_assets(self):
        result=p.compare(CASES,authored_trace(),authored_trace())
        self.assertTrue(result['passed']);self.assertEqual(result['rows'],192)
        self.assertFalse(result['full_vm_state_parity']);self.assertFalse(result['original_content_installation'])
    def test_collision_fault_is_localized_exactly(self):
        trace=authored_trace();result=p.compare(CASES,trace,alter(trace,'collision','query_status',-1))
        self.assertFalse(result['passed'])
        self.assertEqual(result['first_difference'],{'case':'collision','ts1':1,'facing':0,'field':'query_status','expected':10,'actual':-1})
    def test_failed_move_that_mutates_position_is_not_allowed(self):
        trace=authored_trace();result=p.compare(CASES,trace,alter(trace,'collision','after_x',88))
        self.assertFalse(result['passed']);self.assertEqual(result['first_difference']['field'],'after_x')
    def test_two_equal_wrong_results_cannot_claim_coverage(self):
        trace=authored_trace()
        for field,value in [('query_status',-1),('query_unchanged',0),('after_x',88),('retry_status',-1),('recover_x',41),('entities',1)]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                wrong=alter(trace,'collision',field,value);p.compare(CASES,wrong,wrong)
    def test_empty_header_only_truncated_extra_and_reordered_traces_reject(self):
        good=authored_trace();rows=good.splitlines(keepends=True)
        bads=[b'',rows[0],b''.join(rows[:-1]),good+rows[-1],b''.join([rows[0],rows[2],rows[1],*rows[3:]])]
        for bad in bads:
            with self.subTest(length=len(bad)),self.assertRaises(ValueError):p.compare(CASES,good,bad)
    def test_integer_format_and_width_are_strict(self):
        good=authored_trace()
        for value in ['1.0','01','+1','-0','NaN','1e1','9'*100,'32768']:
            with self.subTest(value=value), self.assertRaises(ValueError):p.parse_trace(CASES,alter(good,'collision','after_x',value))
    def test_non_ascii_bom_crlf_and_double_newline_reject(self):
        good=authored_trace()
        for data in [b'\xef\xbb\xbf'+good,good.replace(b'\n',b'\r\n'),good+b'\n',good[:-1],good.replace(b'ground',b'gr\xffund')]:
            with self.assertRaises(ValueError):p.parse_trace(CASES,data)
    def test_fixture_missing_duplicate_or_out_of_order_cases_reject(self):
        rows=CASES.splitlines(keepends=True)
        for data in [b''.join(rows[:-1]),CASES+rows[-1],b''.join([rows[0],rows[2],rows[1],*rows[3:]])]:
            with self.assertRaises(ValueError):p.read_cases(data)
    def test_fixture_flags_heights_booleans_and_directions_reject_invalid_values(self):
        for field,value in [('flags','16384'),('wall_flags','4096'),('heights','-1'),('allow_intersection','2'),('x','1.5')]:
            lines=CASES.decode().splitlines();parts=lines[1].split('\t');idx=lines[0].split('\t').index(field);parts[idx]=value;lines[1]='\t'.join(parts)
            with self.subTest(field=field),self.assertRaises(ValueError):p.read_cases(('\n'.join(lines)+'\n').encode())
        with self.assertRaises(ValueError):p.parse_trace(CASES,alter(authored_trace(),'collision','facing',1))
    def test_changed_required_case_cannot_drop_collision_or_recovery_expectation(self):
        wrong=CASES.replace(b'collision\t88\t56\t3\t0\t1\t0\t10',b'collision\t88\t56\t3\t0\t1\t0\t-1')
        with self.assertRaises(ValueError):p.read_cases(wrong)
    def test_oversized_input_is_rejected(self):
        with self.assertRaises(ValueError):p.parse_trace(CASES,b'x'*(p.MAX_BYTES+1))
    def test_repeated_calls_do_not_change_the_source_or_output(self):
        trace=authored_trace();a=p.compare(CASES,trace,trace);self.assertEqual(a,p.compare(CASES,trace,trace))
    def test_cli_exits_zero_for_match_one_for_drift_two_for_bad_trace(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);cases=root/'cases.tsv';ref=root/'reference.tsv';candidate=root/'candidate.tsv'
            cases.write_bytes(CASES);ref.write_bytes(authored_trace())
            for data,code in [(authored_trace(),0),(alter(authored_trace(),'collision','after_x',88),1),(b'',2)]:
                candidate.write_bytes(data)
                result=subprocess.run([sys.executable,'-B',str(ROOT/'tools/replay/compare_placement.py'),str(cases),str(ref),str(candidate)],capture_output=True,text=True,timeout=5)
                self.assertEqual(result.returncode,code,result.stdout+result.stderr)
if __name__=='__main__':unittest.main()
