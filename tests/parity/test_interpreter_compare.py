"""Strict, adversarial tests for the interpreter evidence boundary."""
import importlib.util
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/replay"))
from compare_interpreter import FIELDS, parse_trace, compare

def valid():
    lines = ["\t".join(FIELDS)]
    for case in ("source4110", "authored"):
        for mode in (1, 0):
            for tick in range(1, 33):
                authored = case == "authored"
                depth = (2 if tick % 3 == 1 else 0) if authored else 0
                row = [case, mode, tick, tick, 123 + tick, 1, 1,
                       0, (11 if tick % 2 else -11) if authored else 0,
                       30 + tick if authored else 30, 40 + tick if authored else 0,
                       depth, 4097 if depth else -1, 4 if depth else -1,
                       0 if depth else -1, 5 if depth else -1]
                lines.append("\t".join(map(str, row)))
    return ("\n".join(lines) + "\n").encode()

def edit(data, row, column, value):
    rows = data.decode().splitlines()
    cells = rows[row + 1].split("\t")
    cells[column] = str(value)
    rows[row + 1] = "\t".join(cells)
    return ("\n".join(rows) + "\n").encode()

class InterpreterEvidence(unittest.TestCase):
    def test_valid_rows_and_pair(self):
        self.assertEqual(len(parse_trace(valid())), 128)
        self.assertTrue(compare(valid(), valid())["passed"])

    def test_localizes_preserved_attribute(self):
        changed = edit(valid(), 6, 9, 31)
        result = compare(valid(), changed)
        self.assertFalse(result["passed"])
        self.assertEqual(result["first_difference"],
                         {"case": "source4110", "ts1": 1, "tick": 7,
                          "field": "attribute_2", "expected": 30, "actual": 31})

    def test_localizes_clock_before_later_differences(self):
        changed = edit(edit(valid(), 6, 3, 6), 6, 9, 31)
        result = compare(valid(), changed)
        self.assertFalse(result["passed"])
        self.assertEqual(result["first_difference"]["field"], "clock_ticks")

    def test_rejects_bad_framing(self):
        for value in (b"", b"\xef\xbb\xbf"+valid(), valid()[:-1],
                      valid()+b"\n", valid().replace(b"\n", b"\r\n"),
                      valid()+b"\xff", b"x"*(1024*1024+1)):
            with self.subTest(length=len(value)), self.assertRaises(ValueError):
                parse_trace(value)

    def test_rejects_row_count_order_and_schema(self):
        rows = valid().splitlines(keepends=True)
        for value in (b"".join(rows[:-1]), b"".join(rows+[rows[-1]]),
                      b"".join(rows[:7]+[rows[6]]+rows[8:]),
                      valid().replace(b"clock_ticks", b"ticks", 1),
                      edit(valid(), 0, 0, "unknown"),
                      edit(valid(), 0, 1, 2)):
            with self.subTest(length=len(value)), self.assertRaises(ValueError):
                parse_trace(value)

    def test_rejects_noncanonical_or_out_of_range_numbers(self):
        for col, value in [(2,"01"), (4,2**64), (4,-1), (7,32768),
                           (7,-32769), (2,"+1"), (3,"1.0"), (3," 1"),
                           (11,9), (12,65536), (13,256), (14,32768)]:
            with self.subTest(column=col, value=value), self.assertRaises(ValueError):
                parse_trace(edit(valid(), 0, col, value))

    def test_rejects_inconsistent_frames(self):
        for col, value in [(12,4097),(13,0),(14,0),(15,0)]:
            with self.subTest(column=col), self.assertRaises(ValueError):
                parse_trace(edit(valid(), 0, col, value))

    def test_equal_but_vacuous_is_not_parity(self):
        data = valid()
        for row in range(64,128):
            for col,value in [(8,11),(9,30),(10,40),(11,0),(12,-1),(13,-1),(14,-1),(15,-1)]:
                data = edit(data,row,col,value)
        with self.assertRaises(ValueError):
            compare(data,data)

    def test_candidate_need_not_match_expected_values_to_be_well_formed(self):
        # Semantic equality belongs to the differential, not parser rewriting.
        changed = edit(valid(), 6, 9, 31)
        self.assertEqual(len(parse_trace(changed)),128)
        self.assertFalse(compare(valid(),changed)["passed"])

if __name__ == "__main__":
    unittest.main()
