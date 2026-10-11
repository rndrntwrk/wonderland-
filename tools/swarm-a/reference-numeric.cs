// Extracted arithmetic/RNG probe for FreeSO commit
// 4c6b3e8f5835b228723caea3c9f683c62f244f73.
// This is a small C# runtime probe, NOT a full FreeSO reference oracle.
// Source expressions: VMContext.cs:524, Primitives/VMExpression.cs:59,
// Engine/VMMemory.cs:653, Primitives/VMRandomNumber.cs:9.
// Run: mcs -checked- -out:/tmp/reference-numeric.exe reference-numeric.cs
//      mono /tmp/reference-numeric.exe
using System;
using System.Globalization;

public static class ReferenceNumeric
{
    private static void Integer(string name, Func<int> operation)
    {
        try { Console.WriteLine(name + "\t" + operation().ToString(CultureInfo.InvariantCulture)); }
        catch (Exception error) { Console.WriteLine(name + "\t" + error.GetType().Name); }
    }

    private static int Divide(int lhs, int rhs) { return rhs == 0 ? -1 : lhs / rhs; }
    private static int Modulo(int lhs, int rhs) { return rhs == 0 ? lhs : ((lhs % rhs + rhs) % rhs); }
    private static int Remainder(int lhs, int rhs) { return lhs % rhs; }
    private static int NarrowShort(double value) { return (short)value; }
    private static int NarrowInt(double value) { return (int)value; }

    private static ulong NextRandom(ref ulong seed, ulong max)
    {
        if (max == 0) return 0;
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        return (seed * 2685821657736338717UL) % max;
    }

    public static void Main()
    {
        Integer("division_zero", () => Divide(17, 0));
        Integer("division_negative", () => Divide(-7, 3));
        Integer("division_overflow", () => Divide(int.MinValue, -1));
        Integer("remainder_overflow", () => Remainder(int.MinValue, -1));
        Integer("modulo_zero", () => Modulo(-7, 0));
        Integer("modulo_positive", () => Modulo(-7, 3));
        Integer("modulo_negative", () => Modulo(7, -3));
        Integer("modulo_addition_overflow", () => Modulo(1, int.MaxValue));
        Integer("modulo_division_overflow", () => Modulo(int.MinValue, -1));
        foreach (double value in new double[] {
            -0.0, -0.5, 0.5, 1.5, 2.5, -1.5, -2.5,
            32767.9, 32768.0, 46340.0, 65535.0, 65536.0,
            -32769.0, 2147483647.0, 2147483648.0, -2147483649.0,
            double.NaN, double.PositiveInfinity, double.NegativeInfinity })
        {
            var input = value;
            var key = BitConverter.DoubleToInt64Bits(value).ToString("X16", CultureInfo.InvariantCulture);
            Integer("f64_i16_" + key, () => NarrowShort(input));
            Integer("f64_i32_" + key, () => NarrowInt(input));
            Console.WriteLine("round_" + key + "\t" + BitConverter.DoubleToInt64Bits(Math.Round(value)).ToString("X16", CultureInfo.InvariantCulture));
        }
        foreach (int value in new int[] { -1, 0, 1, 2, 15, 16, int.MaxValue })
        {
            var input = value;
            Integer("sqrt_" + value, () => (short)Math.Sqrt(input));
        }
        foreach (ulong start in new ulong[] { 0, 1, ulong.MaxValue, 0x123456789ABCDEF0UL })
        {
            ulong seed = start;
            foreach (ulong bound in new ulong[] { 0, 1, 100, 65535, 1 })
            {
                ulong result = NextRandom(ref seed, bound);
                Console.WriteLine("rng_" + start + "_" + bound + "\t" + seed + "\t" + result);
            }
        }
    }
}
