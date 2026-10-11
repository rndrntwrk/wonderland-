# SPDX-License-Identifier: MPL-2.0
# Test-only CPU shims; loaded by sprite-reader-oracle.py with pinned methods.
shim = r'''
using System;
using System.IO;
class IffChunk { public IffFile ChunkParent; public virtual void Read(IffFile iff, Stream stream) {} }
class IffFile {
    public static bool RETAIN_CHUNK_DATA = true;
    PALT palette = new PALT();
    public IffFile() { palette.Colors = new Color[300]; for (int i=0; i<300; i++) palette.Colors[i] = new Color((byte)(i*13+(i/256)*123), (byte)(i*7), (byte)(i*3), 255); }
    public T Get<T>(ushort id) where T:class { return palette as T; }
}
class PALT { public Color[] Colors; public int References; }
struct Color { public byte R,G,B,A; public Color(byte r,byte g,byte b,byte a) { R=r;G=g;B=b;A=a; } }
struct Vector2 { public float X,Y; public Vector2(float x, float y) {X=x;Y=y;} }
enum ByteOrder { LITTLE_ENDIAN }
class IoBuffer : IDisposable {
    BinaryReader r;
    public static IoBuffer FromStream(Stream s, ByteOrder o) { return new IoBuffer { r = new BinaryReader(s) }; }
    public bool HasMore { get { return r.BaseStream.Position < r.BaseStream.Length; } }
    public uint ReadUInt32() { return r.ReadUInt32(); }
    public ushort ReadUInt16() { return r.ReadUInt16(); }
    public short ReadInt16() { return r.ReadInt16(); }
    public byte ReadByte() { return r.ReadByte(); }
    public byte[] ReadBytes(uint n) { return r.ReadBytes(checked((int)n)); }
    public void Seek(SeekOrigin o, uint n) { r.BaseStream.Seek(n, o); }
    public void Dispose() { r.Dispose(); }
}
'''
spr2 = '''class SPR2 : IffChunk { public SPR2Frame[] Frames; public uint DefaultPaletteID; public int FloorCopy = 0; public bool ZAsAlpha = false;\n'''+methods[0]+'''}\n'''
frame = '''class SPR2Frame { public int Width,Height; public uint Flags; public ushort PaletteID,TransparentColorIndex; public Vector2 Position; public Color[] PixelData; public byte[] PalData,ZBufferData; private uint Version; private byte[] ToDecode; private SPR2 Parent; public SPR2Frame(SPR2 p) { Parent=p; } private void CopyZToAlpha() { throw new Exception("unexpected renderer path"); } private void FloorCopy() { throw new Exception("unexpected renderer path"); } private void FloorCopyWater() { throw new Exception("unexpected renderer path"); }\n'''+ '\n'.join(methods[1:])+'''}\n'''
main = r'''
class Probe {
    static void Equal(long got, long want, string label) { if (got != want) throw new Exception(label+" actual="+got+" expected="+want); }
    static void Main(string[] args) {
        int frames = 0; long pixels=0;
        using (var data = new BinaryReader(File.OpenRead(args[0]))) {
            uint cases = data.ReadUInt32();
            for (int c=0; c<cases; c++) {
                var bytes = data.ReadBytes(checked((int)data.ReadUInt32()));
                var iff = new IffFile(); var set = new SPR2 { ChunkParent=iff };
                set.Read(iff, new MemoryStream(bytes));
                Equal(set.DefaultPaletteID, data.ReadUInt32(), "default palette");
                Equal(set.Frames.Length, data.ReadUInt32(), "frame count");
                foreach (var f in set.Frames) {
                    Equal(f.Width, data.ReadUInt32(), "width"); Equal(f.Height, data.ReadUInt32(), "height");
                    Equal(f.Flags, data.ReadUInt32(), "flags"); Equal(f.PaletteID, data.ReadUInt32(), "effective palette"); Equal(f.TransparentColorIndex, data.ReadUInt32(), "transparent index");
                    Equal((int)f.Position.X, data.ReadInt16(), "position X"); Equal((int)f.Position.Y, data.ReadInt16(), "position Y");
                    foreach (var color in f.PixelData) { Equal(color.R, data.ReadByte(), "R"); Equal(color.G, data.ReadByte(), "G"); Equal(color.B, data.ReadByte(), "B"); Equal(color.A, data.ReadByte(), "A"); }
                    foreach (var index in f.PalData) Equal(index, data.ReadByte(), "palette index");
                    if (f.ZBufferData != null) foreach (var z in f.ZBufferData) Equal(z, data.ReadByte(), "depth");
                    frames++; pixels += f.PixelData.Length;
                }
            }
            Equal(data.BaseStream.Position, data.BaseStream.Length, "input fully consumed");
            Console.WriteLine("PASS: "+cases+" cases, "+frames+" frames, "+pixels+" pixels decoded with source SPR2 reader methods");
        }
    }
}
'''
source = shim + spr2 + frame + main
