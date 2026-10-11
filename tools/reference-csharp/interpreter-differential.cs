// F-owned driver linked to the complete original assembly.
// Only content-provider/metadata inputs are authored; VM methods are not copied.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.Serialization;
using System.Security.Cryptography;
using System.Text;
using FSO.Content;
using FSO.Files.Formats.IFF;
using FSO.Files.Formats.IFF.Chunks;
using FSO.SimAntics;
using FSO.SimAntics.Engine;
using FSO.SimAntics.Entities;

internal static class InterpreterDifferential
{
    private const string OriginalHash = "20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865";
    private const string AuthoredHash = "4d52ba85e96cc6f87d6f14e570837ca7d32a3809123894f93f8f071cff725579";
    private const string Header = "case\tts1\ttick\tclock_ticks\trng\tentities\tobject_id\tattribute_0\tattribute_1\tattribute_2\tattribute_3\tstack_count\troutine\tpc\targ0\tlocal0";

    private static void Require(bool ok, string message)
    {
        if (!ok) throw new InvalidOperationException(message);
    }
    private static string Hash(byte[] value)
    {
        using (var hash = SHA256.Create())
            return BitConverter.ToString(hash.ComputeHash(value)).Replace("-", "").ToLowerInvariant();
    }
    private static Content Configure(bool ts1)
    {
        var content = (Content)FormatterServices.GetUninitializedObject(typeof(Content));
        content.TS1 = ts1;
        content.BasePath = Directory.GetCurrentDirectory();
        var globalsIff = new IffFile(); globalsIff.MarkThrowaway();
        var globals = new WorldGlobalProvider(content);
        var cache = typeof(WorldGlobalProvider).GetField("Cache", BindingFlags.Instance | BindingFlags.NonPublic);
        Require(cache != null, "content cache seam changed");
        cache.SetValue(globals, new Dictionary<string, GameGlobal> {
            { "global", new GameGlobal { Resource = new GameGlobalResource(globalsIff, null) } }
        });
        content.WorldObjectGlobals = globals;
        var singleton = typeof(Content).GetField("INSTANCE", BindingFlags.Static | BindingFlags.NonPublic);
        Require(singleton != null, "content singleton seam changed");
        singleton.SetValue(null, content);
        return content;
    }
    private static IffFile ReadInput(string root, bool authored)
    {
        byte[] bytes;
        if (authored)
        {
            var text = File.ReadAllText(Path.Combine(root, "fixtures/reference/interpreter-authored.iff.hex"));
            Require(text.Length == 745 && text[744] == '\n', "authored fixture framing");
            bytes = new byte[372];
            for (int i = 0; i < bytes.Length; i++)
                bytes[i] = byte.Parse(text.Substring(i * 2, 2), NumberStyles.HexNumber, CultureInfo.InvariantCulture);
        }
        else bytes = File.ReadAllBytes(Path.Combine(root, "TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff"));
        Require(Hash(bytes) == (authored ? AuthoredHash : OriginalHash), "input resource hash mismatch");
        var result = new IffFile(); result.MarkThrowaway();
        using (var input = new MemoryStream(bytes)) result.Read(input);
        return result;
    }
    private static void Run(string root, bool authored, bool ts1)
    {
        VM.UseWorld = false;
        VMContext.InitVMConfig(ts1);
        VMContext.BindAssembler();
        var content = Configure(ts1);
        var context = new VMContext(null);
        var driver = new FixtureDriver(); // reused #44 driver; original InternalTick
        var vm = new VM(context, driver, null);
        int diagnostics = 0;
        vm.OnDialog += delegate { diagnostics++; };
        vm.OnChatEvent += delegate { diagnostics++; };
        vm.Init();
        context.Architecture = new VMArchitecture(8, 8, null, context);
        context.Clock.UTCStart = 630822816000000000L;
        context.RandomSeed = 0x123456789abcdef0UL;
        Require(vm.GetType() == typeof(VM) && typeof(VM).Assembly == typeof(VMThread).Assembly, "original assembly identity");
        Require(vm.GlobalLink == null && vm.EODHost == null, "no production effect provider");
        var input = ReadInput(root, authored);
        var local = new IffFile(); local.MarkThrowaway();
        foreach (ushort id in authored ? new ushort[] { 4096, 4097 } : new ushort[] { 4110 })
        {
            var behavior = input.Get<BHAV>(id);
            Require(behavior != null, "declared original-format behavior required");
            local.AddChunk(behavior);
        }
        var definition = new OBJD {
            GUID = 0xf00d0044, ObjectType = OBJDType.Normal, NumAttributes = 4,
            StackSize = 8, BHAV_Init = (ushort)(authored ? 0 : 4110),
            BHAV_MainID = (ushort)(authored ? 4096 : 4110),
            ChunkLabel = "Declared differential fixture", ChunkProcessed = true
        };
        var resource = new GameObjectResource(local, null, null, "swarm-f-interpreter", content);
        var entity = new VMGameObject(new GameObject { GUID = definition.GUID, OBJ = definition, Resource = resource }, null);
        for (int i = 0; i < 4; i++) entity.SetAttribute(i, (short)((i + 1) * 10));
        entity.MultitileGroup = new VMMultitileGroup(); entity.MultitileGroup.AddObject(entity);
        vm.AddEntity(entity); entity.Init(context);
        Require(entity.Thread.GetType() == typeof(VMThread), "original thread");
        for (int tick = 1; tick <= 32; tick++)
        {
            if (tick > 1)
            {
                if (authored) entity.SetAttribute(0, (short)(tick % 8 < 4 ? tick : -tick));
                else
                {
                    entity.SetAttribute(0, (short)-tick);
                    entity.SetAttribute(1, (short)(100 + tick));
                    entity.SetAttribute(3, (short)(200 + tick));
                }
            }
            vm.Tick();
            Require(diagnostics == 0 && !vm.Aborting && !entity.Dead, "unexpected original runtime diagnostic");
            Require(vm.Ready && driver.LastTick == tick, "original driver did not complete");
            var thread = entity.Thread;
            var top = thread.Stack.Count == 0 ? null : thread.Stack[thread.Stack.Count - 1];
            Console.WriteLine(string.Join("\t", new object[] {
                authored ? "authored" : "source4110", ts1 ? 1 : 0, tick,
                context.Clock.Ticks, context.RandomSeed, vm.Entities.Count, entity.ObjectID,
                entity.GetAttribute(0), entity.GetAttribute(1), entity.GetAttribute(2), entity.GetAttribute(3),
                thread.Stack.Count, top == null ? -1 : top.Routine.ID,
                top == null ? -1 : top.InstructionPointer,
                top == null || top.Args == null || top.Args.Length == 0 ? -1 : top.Args[0],
                top == null || top.Locals == null || top.Locals.Length == 0 ? -1 : top.Locals[0]
            }));
        }
    }
    public static int Main(string[] args)
    {
        try
        {
            Require(args.Length == 2, "supply source root and a new runtime-identity path");
            Require(Environment.GetEnvironmentVariable("WONDERLAND_INTERPRETER_TEST_ONLY") == "1", "explicit interpreter test opt-in required");
            CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
            CultureInfo.DefaultThreadCurrentUICulture = CultureInfo.InvariantCulture;
            Console.WriteLine(Header);
            foreach (bool authored in new bool[] { false, true })
                foreach (bool ts1 in new bool[] { true, false }) Run(args[0], authored, ts1);
            using (var file = new FileStream(args[1], FileMode.CreateNew))
            using (var writer = new StreamWriter(file, new UTF8Encoding(false)))
            {
                writer.NewLine = "\n";
                writer.WriteLine("clr\t" + Environment.Version);
                writer.WriteLine("vm_type\t" + typeof(VM).FullName);
                writer.WriteLine("thread_type\t" + typeof(VMThread).FullName);
                writer.WriteLine("vm_sha256\t" + Hash(File.ReadAllBytes(typeof(VM).Assembly.Location)));
                writer.WriteLine("witness_sha256\t" + Hash(File.ReadAllBytes(Assembly.GetExecutingAssembly().Location)));
                writer.WriteLine("source_sha256\t" + OriginalHash);
                writer.WriteLine("authored_sha256\t" + AuthoredHash);
                writer.WriteLine("provider\tauthored-in-memory");
            }
            return 0;
        }
        catch (Exception error) { Console.Error.WriteLine(error); return 1; }
    }
}
