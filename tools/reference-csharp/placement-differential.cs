// F-only original-assembly placement witness. The geometry and physical metadata
// are declared fixtures, not installed game objects. No VM placement code is copied.
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
using FSO.LotView.Model;
using FSO.SimAntics;
using FSO.SimAntics.Engine;
using FSO.SimAntics.Entities;
using FSO.SimAntics.Model;
using Microsoft.Xna.Framework;

internal static class PlacementDifferential
{
    private const string CasesHash = "602716fdff80c06454707fbe4ed4d2194f0c4dacf292cfb786ba5655d7a33f13";
    private const string Header = "case\tts1\tfacing\tquery_status\tquery_blocker\tcommit_status\tcommit_blocker\tquery_unchanged\tafter_x\tafter_y\tafter_level\tafter_facing\tblocker_x\tblocker_y\tblocker_level\tblocker_facing\tentities\trecover_status\trecover_x\trecover_y\tretry_status\tretry_blocker\tfinal_x\tfinal_y\tfinal_facing";
    private static void Require(bool ok, string message)
    {
        if (!ok) throw new InvalidOperationException(message);
    }
    private static string Hash(byte[] bytes)
    {
        using (var hash = SHA256.Create()) return BitConverter.ToString(hash.ComputeHash(bytes)).Replace("-", "").ToLowerInvariant();
    }
    private static Content Configure(bool ts1)
    {
        // Identical declared provider seam to #44/#45. This is the only
        // uninitialized object: all VM, architecture, entity and thread constructors run.
        var content = (Content)FormatterServices.GetUninitializedObject(typeof(Content));
        content.TS1 = ts1;
        content.BasePath = Directory.GetCurrentDirectory();
        var empty = new IffFile(); empty.MarkThrowaway();
        var globals = new WorldGlobalProvider(content);
        var cache = typeof(WorldGlobalProvider).GetField("Cache", BindingFlags.Instance | BindingFlags.NonPublic);
        Require(cache != null, "content-provider cache seam changed");
        cache.SetValue(globals, new Dictionary<string, GameGlobal> {
            { "global", new GameGlobal { Resource = new GameGlobalResource(empty, null) } }
        });
        content.WorldObjectGlobals = globals;
        var singleton = typeof(Content).GetField("INSTANCE", BindingFlags.Static | BindingFlags.NonPublic);
        Require(singleton != null, "content singleton seam changed");
        singleton.SetValue(null, content);
        return content;
    }
    private static void Rules(VMGameObject entity, short flags, short wall, short heights)
    {
        Require(entity.SetValue(VMStackObjectVariable.PlacementFlags, flags), "physical placement flags rejected");
        Require(entity.SetValue(VMStackObjectVariable.WallPlacementFlags, wall), "wall flags rejected");
        Require(entity.SetValue(VMStackObjectVariable.AllowedHeightFlags, heights), "height flags rejected");
    }
    private static VMGameObject Object(VM vm, Content content, short id, short x, short y)
    {
        var definition = new OBJD {
            GUID = (uint)(0xf00d4600 + id), ObjectType = OBJDType.Normal,
            NumAttributes = 0, StackSize = 8, TileWidth = 16, FootprintMask = 0,
            ChunkLabel = "Declared placement metadata", ChunkProcessed = true
        };
        var local = new IffFile(); local.MarkThrowaway();
        var resource = new GameObjectResource(local, null, null, "swarm-f-placement-" + id, content);
        var entity = new VMGameObject(new GameObject { GUID = definition.GUID, OBJ = definition, Resource = resource }, null);
        entity.MultitileGroup = new VMMultitileGroup(); entity.MultitileGroup.AddObject(entity);
        vm.AddEntity(entity); entity.Init(vm.Context);
        Require(entity.ObjectID == id && entity.Thread.GetType() == typeof(VMThread), "original entity/thread identity");
        Rules(entity, 3, 0, 1);
        var placed = entity.SetPosition(new LotTilePos(x, y, 1), Direction.NORTH, vm.Context);
        Require(placed.Status == VMPlacementError.Success, "initial original placement failed: " + placed.Status);
        return entity;
    }
    private static int DirectionIndex(Direction direction)
    {
        for (int i = 0; i < 8; i++) if ((int)direction == (1 << i)) return i;
        throw new InvalidOperationException("invalid original direction mask");
    }
    private static int[] Pose(VMGameObject entity)
    {
        return new int[] { entity.Position.x, entity.Position.y, entity.Position.Level, DirectionIndex(entity.Direction) };
    }
    private static string QueryState(VM vm, VMGameObject a, VMGameObject b)
    {
        // Scoped observable query purity, not a full C# VM-state snapshot.
        return string.Join(",", Pose(a)) + ";" + string.Join(",", Pose(b)) + ";" +
            vm.Context.RandomSeed + ";" + vm.Entities.Count + ";" +
            a.GetValue(VMStackObjectVariable.PlacementFlags) + ";" +
            a.GetValue(VMStackObjectVariable.WallPlacementFlags) + ";" +
            a.GetValue(VMStackObjectVariable.AllowedHeightFlags);
    }
    private static int Blocker(VMPlacementResult result) { return result.Object == null ? 0 : result.Object.ObjectID; }
    private static void Run(string[] values, bool ts1, int notch)
    {
        VM.UseWorld = false;
        VMContext.InitVMConfig(ts1);
        VMContext.BindAssembler();
        var content = Configure(ts1);
        var context = new VMContext(null);
        var vm = new VM(context, new FixtureDriver(), null);
        int diagnostics = 0;
        vm.OnDialog += delegate { diagnostics++; };
        vm.OnChatEvent += delegate { diagnostics++; };
        vm.Init();
        Require(vm.TS1 == ts1 && vm.GlobalLink == null && vm.EODHost == null, "invalid original mode/provider");
        Require(vm.GlobalState.Length == 38 && vm.GetGlobalValue(20) == 255, "original VM initialization missing");
        var architecture = new VMArchitecture(8, 8, null, context);
        context.Architecture = architecture;
        architecture.UpdateBuildableArea(new Rectangle(0, 0, 8, 8), 1);
        Require(architecture.SetFloor(3, 5, 1, new FloorTile { Pattern = 1 }, true), "fixture floor setup");
        Require(architecture.SetFloor(5, 5, 1, new FloorTile { Pattern = 65534 }, true), "fixture water setup");
        Require(architecture.SetFloor(6, 5, 1, new FloorTile { Pattern = 65535 }, true), "fixture pool setup");
        architecture.Tick(); // original room map/obstacle indexes, no fabricated cache
        context.RandomSeed = 0x123456789abcdef0UL;
        var a = Object(vm, content, 1, 40, 40);
        var b = Object(vm, content, 2, 88, 56);
        Rules(a, short.Parse(values[3]), short.Parse(values[4]), short.Parse(values[5]));
        var target = new LotTilePos(short.Parse(values[1]), short.Parse(values[2]), 1);
        var facing = (Direction)(1 << notch);
        var flags = values[6] == "1" ? VMPlaceRequestFlags.AllowIntersection : VMPlaceRequestFlags.Default;
        var before = QueryState(vm, a, b);
        var query = a.PositionValid(target, facing, context, flags);
        var pure = before == QueryState(vm, a, b) ? 1 : 0;
        var committed = a.SetPosition(target, facing, context, flags);
        var after = Pose(a); var other = Pose(b); var entities = vm.Entities.Count;
        Rules(a, 3, 0, 1);
        var recovered = a.SetPosition(new LotTilePos(40, 72, 1), Direction.NORTH, context);
        var recovery = Pose(a);
        var retry = a.SetPosition(new LotTilePos(88, 56, 1), Direction.NORTH, context);
        var final = Pose(a);
        Require(diagnostics == 0 && !vm.Aborting && !a.Dead && !b.Dead, "unexpected original placement diagnostic");
        Require(context.RandomSeed == 0x123456789abcdef0UL, "unscheduled placement consumed RNG");
        // The expected_status column is deliberately NOT used to produce observations.
        Console.WriteLine(string.Join("\t", new object[] {
            values[0], ts1 ? 1 : 0, notch, (int)query.Status, Blocker(query), (int)committed.Status,
            Blocker(committed), pure, after[0],after[1],after[2],after[3],other[0],other[1],other[2],other[3],
            entities, (int)recovered.Status,recovery[0],recovery[1],(int)retry.Status,Blocker(retry),
            final[0],final[1],final[3]
        }));
    }
    public static int Main(string[] args)
    {
        try {
            Require(args.Length == 2, "supply source root and a new runtime-identity path");
            Require(Environment.GetEnvironmentVariable("WONDERLAND_PLACEMENT_TEST_ONLY") == "1", "explicit placement test opt-in required");
            CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
            CultureInfo.DefaultThreadCurrentUICulture = CultureInfo.InvariantCulture;
            var bytes = File.ReadAllBytes(Path.Combine(args[0], "fixtures/reference/placement-cases.tsv"));
            Require(Hash(bytes) == CasesHash, "placement fixture identity changed");
            var lines = Encoding.ASCII.GetString(bytes).TrimEnd('\n').Split('\n');
            Require(lines.Length == 25, "placement case count");
            Console.WriteLine(Header);
            for (int i = 1; i < lines.Length; i++) {
                var values = lines[i].Split('\t'); Require(values.Length == 8, "placement column count");
                foreach (bool ts1 in new bool[] { true, false })
                    foreach (int notch in new int[] { 0, 2, 4, 6 }) Run(values, ts1, notch);
            }
            using (var file = new FileStream(args[1], FileMode.CreateNew))
            using (var writer = new StreamWriter(file, new UTF8Encoding(false))) {
                writer.NewLine = "\n";
                writer.WriteLine("clr\t" + Environment.Version);
                writer.WriteLine("vm_type\t" + typeof(VM).FullName);
                writer.WriteLine("entity_type\t" + typeof(VMGameObject).FullName);
                writer.WriteLine("vm_sha256\t" + Hash(File.ReadAllBytes(typeof(VM).Assembly.Location)));
                writer.WriteLine("witness_sha256\t" + Hash(File.ReadAllBytes(Assembly.GetExecutingAssembly().Location)));
                writer.WriteLine("cases_sha256\t" + CasesHash);
                writer.WriteLine("provider\tauthored-two-object-ground-geometry");
            }
            return 0;
        } catch (Exception error) { Console.Error.WriteLine(error); return 1; }
    }
}
