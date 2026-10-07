"""Authored comparator fixtures, not original-engine execution evidence."""
from pathlib import Path
import json
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
CASES = ('case\tts1\tseed\tutc_start\tstart_tick\tfraction\tminutes\thours\tday\tmonth\tyear\tfire\tsteps\n'
         'smoke\t0\t0\t0\t0\t0\t0\t0\t1\t6\t1997\t20000\t2\n')
HEADER = 'case\tstep\tclock_tick\trng_before\tbound\trandom\tbranch\textra\trng_after\tfraction\tminutes\thours\tday\tmonth\tyear\tfire\tseconds\tutc\n'
ROWS = [
    ['smoke','1','1','0','0','0','0','0','0','1','0','0','1','6','1997','20000','0','330000'],
    ['smoke','2','2','0','1','0','0','0','0','2','0','0','1','6','1997','20000','0','670000'],
]

def trace(rows=ROWS):
    return HEADER + ''.join('\t'.join(row)+'\n' for row in rows)


class TraceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.cases = self.put('cases.tsv', CASES)
        self.ref = self.put('reference.tsv', trace())
        self.other = self.put('candidate.tsv', trace())
        self.report = self.work/'report.json'

    def put(self, name, contents):
        path = self.work/name
        path.write_bytes(contents if isinstance(contents,bytes) else contents.encode())
        return path

    def run_gate(self, expected):
        result = subprocess.run([sys.executable,'-B',str(ROOT/'tools/replay/compare_reference.py'),
            '--cases',str(self.cases),'--reference',str(self.ref),'--candidate',str(self.other),
            '--output',str(self.report)],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,expected,result.stdout+'\n'+result.stderr)
        value = json.loads(result.stdout)
        self.assertEqual(value['schema'],1)
        self.assertEqual(value['full_original_engine'],'not-tested')
        if self.report.exists():
            self.assertEqual(json.loads(self.report.read_text()),value)
        return value

    def edit(self,index,field,value,both=False):
        rows=[list(row) for row in ROWS]
        columns=HEADER.strip().split('\t')
        rows[index][columns.index(field)]=value
        self.put('candidate.tsv',trace(rows))
        if both:self.put('reference.tsv',trace(rows))

    def test_equal_complete_records_pass_with_exact_input_and_output_hashes(self):
        import hashlib
        result=self.run_gate(0)
        self.assertTrue(result['passed'])
        self.assertEqual(result['records'],2)
        self.assertEqual(result['compared_fields'],32)
        self.assertIsNone(result['first_difference'])
        self.assertEqual(result['input_sha256'],hashlib.sha256(self.cases.read_bytes()).hexdigest())
        self.assertEqual(result['reference_sha256'],result['candidate_sha256'])
        self.assertEqual(result['evidence_scope'],'supplied-traces-not-execution-attestation')

    def test_first_different_tick_and_field_are_localized(self):
        self.edit(1,'fire','19999')
        result=self.run_gate(1)
        self.assertFalse(result['passed'])
        self.assertEqual(result['first_difference'],{'case':'smoke','step':2,'clock_tick':'2','field':'fire','reference':'20000','candidate':'19999'})

    def test_rng_difference_is_not_hidden_by_matching_final_clock(self):
        self.edit(1,'rng_after','7')
        result=self.run_gate(1)
        self.assertEqual(result['first_difference']['field'],'rng_after')
        self.assertEqual(result['first_difference']['step'],2)

    def test_branch_difference_is_localized_before_its_later_state_effect(self):
        self.edit(1,'branch','1')
        self.assertEqual(self.run_gate(1)['first_difference']['field'],'branch')

    def test_earliest_difference_not_final_result_is_reported(self):
        rows=[list(row) for row in ROWS]
        rows[0][15]='19998';rows[1][15]='19999'
        self.put('candidate.tsv',trace(rows))
        self.assertEqual(self.run_gate(1)['first_difference']['step'],1)

    def test_both_traces_empty_are_not_false_equivalence(self):
        self.put('reference.tsv',HEADER);self.put('candidate.tsv',HEADER)
        self.assertFalse(self.run_gate(2)['passed'])

    def test_matching_truncated_traces_are_rejected(self):
        self.put('reference.tsv',trace(ROWS[:1]));self.put('candidate.tsv',trace(ROWS[:1]))
        self.run_gate(2)

    def test_extra_and_duplicate_steps_are_rejected(self):
        self.put('candidate.tsv',trace(ROWS+ROWS[1:]))
        self.run_gate(2)

    def test_reordered_steps_are_rejected(self):
        self.put('candidate.tsv',trace(list(reversed(ROWS))))
        self.run_gate(2)

    def test_unknown_case_cannot_be_compared_as_original_evidence(self):
        self.edit(0,'case','other',both=True)
        self.run_gate(2)

    def test_wrong_field_order_is_not_normalized_away(self):
        self.put('candidate.tsv',trace().replace('branch\textra','extra\tbranch',1))
        self.run_gate(2)

    def test_extra_column_is_rejected(self):
        self.put('candidate.tsv',trace().replace('rng_after\t','surprise\trng_after\t',1))
        self.run_gate(2)

    def test_missing_final_newline_is_not_a_complete_record(self):
        self.put('candidate.tsv',trace().rstrip('\n'))
        self.run_gate(2)

    def test_noncanonical_number_is_rejected(self):
        self.edit(0,'rng_after','00',both=True)
        self.run_gate(2)

    def test_scientific_notation_and_precision_loss_are_not_admitted(self):
        self.edit(1,'rng_after','9.007199254740993e15',both=True)
        self.run_gate(2)

    def test_out_of_range_u64_is_rejected(self):
        self.edit(1,'rng_after',str(1<<64),both=True)
        self.run_gate(2)

    def test_u64_above_javascript_safe_integer_is_compared_exactly(self):
        self.edit(1,'rng_after','9007199254740993')
        result=self.run_gate(1)
        self.assertEqual(result['first_difference']['candidate'],'9007199254740993')

    def test_wrong_clock_tick_fails_even_in_equal_traces(self):
        self.edit(0,'clock_tick','4',both=True)
        self.run_gate(2)

    def test_rng_before_must_match_the_previous_committed_row(self):
        self.edit(1,'rng_before','1',both=True)
        self.run_gate(2)

    def test_wrong_bound_and_impossible_result_are_rejected(self):
        self.edit(0,'bound','6',both=True)
        self.run_gate(2)

    def test_bound_zero_result_must_be_zero(self):
        self.edit(0,'random','1',both=True)
        self.run_gate(2)

    def test_invalid_utf8_and_control_bytes_are_rejected(self):
        self.put('candidate.tsv',b'\xff\x00')
        self.run_gate(2)

    def test_duplicate_case_declarations_are_rejected(self):
        self.put('cases.tsv',CASES+CASES.split('\n')[1]+'\n')
        self.run_gate(2)

    def test_zero_length_scenarios_are_not_coverage(self):
        self.put('cases.tsv',CASES.replace('\t20000\t2\n','\t20000\t0\n'))
        self.run_gate(2)

    def test_case_path_syntax_is_not_an_executable_or_output_path(self):
        self.put('cases.tsv',CASES.replace('smoke\t','../smoke\t'))
        self.run_gate(2)

    def test_trace_size_is_bounded(self):
        self.put('candidate.tsv',b'x'*(4*1024*1024+1))
        self.run_gate(2)

    def test_input_symlinks_are_rejected(self):
        self.other.unlink();self.other.symlink_to(self.ref)
        self.run_gate(2)

    def test_existing_report_is_not_overwritten(self):
        self.report.write_text('retain this evidence')
        result=subprocess.run([sys.executable,'-B',str(ROOT/'tools/replay/compare_reference.py'),
            '--cases',str(self.cases),'--reference',str(self.ref),'--candidate',str(self.other),
            '--output',str(self.report)],capture_output=True,text=True,timeout=10)
        self.assertEqual(result.returncode,2,result.stdout+result.stderr)
        self.assertEqual(self.report.read_text(),'retain this evidence')


if __name__=='__main__':unittest.main()
