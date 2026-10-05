using System;
using System.Globalization;
using System.IO;
using System.Text;
using FSO.Files.Utils;

class SourceTextProbe {
    static void Main() {
        foreach (var token in new[] {
            "-0", "-0.0", "-1e-999", "-1e-45", "1.17549435e-38",
            "1.0000000596046448", "1,234.5", "1,,2", "12,34", "1,",
            "\t1.25\t", "3.4028235677973366e38"
        }) {
            try {
                using (var input = new MemoryStream(Encoding.UTF8.GetBytes(token + "\n")))
                using (var source = new BCFReadString(input, false)) {
                    var value = source.ReadFloat();
                    Console.WriteLine(token + ":" + BitConverter.ToUInt32(BitConverter.GetBytes(value), 0).ToString("x8"));
                }
            } catch (Exception e) { Console.WriteLine(token + ":" + e.GetType().Name); }
        }
        foreach (var n in new[] {0, 1, 125, 126, 127, 251, 252}) {
            float delta = (float)(3.9676e-10 * (Math.Pow((double)n - 126, 3) * Math.Abs(n - 126)));
            Console.WriteLine("delta:" + n + ":" + BitConverter.ToUInt32(BitConverter.GetBytes(delta), 0).ToString("x8"));
        }
    }
}
