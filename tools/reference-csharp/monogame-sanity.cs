// Execution gate for the actual MonoGame dependency used by the VM cohort.
using System;
using Microsoft.Xna.Framework;
internal static class MonoGameSanity
{
    private static int Main()
    {
        Console.WriteLine(typeof(Point).Assembly.FullName);
        Console.WriteLine(typeof(Point).Assembly.Location);
        var point = new Point(3, 5);
        var rectangle = new Rectangle(10, 20, 30, 40);
        var vector = Vector2.Transform(new Vector2(2, 3), Matrix.CreateTranslation(4, 5, 0));
        Console.WriteLine("point={0},{1}; rectangle={2},{3},{4},{5}; vector={6},{7}",
            point.X, point.Y, rectangle.X, rectangle.Y, rectangle.Width, rectangle.Height, vector.X, vector.Y);
        return point.X == 3 && point.Y == 5 && rectangle.X == 10 && rectangle.Y == 20 &&
            rectangle.Width == 30 && rectangle.Height == 40 && rectangle.Contains(15, 25) &&
            vector.X == 6 && vector.Y == 8 ? 0 : 1;
    }
}
