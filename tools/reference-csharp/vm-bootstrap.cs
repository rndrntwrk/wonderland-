// Original-assembly witness. No simulation class or primitive is copied.
// Content installation is replaced by an explicitly authored provider cohort.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.Serialization;
using System.Security.Cryptography;
using FSO.Content;
using FSO.Files.Formats.IFF;
using FSO.Files.Formats.IFF.Chunks;
using FSO.SimAntics;
using FSO.SimAntics.Engine;
using FSO.SimAntics.Entities;
using FSO.SimAntics.NetPlay;
using FSO.SimAntics.NetPlay.Model;
using FSO.SimAntics.NetPlay.Model.Commands;

internal sealed class FixtureDriver : VMNetDriver
{
    private uint next;
    public override bool Tick(VM vm)
    {
        InternalTick(vm, new VMNetTick {
            TickID = ++next, RandomSeed = vm.Context.RandomSeed,
            Commands = new List<VMNetCommand>()
        });
        return true;
    }
    public override void SendCommand(VMNetCommandBodyAbstract command) { throw new InvalidOperationException("Unexpected external command"); }
    public override void SendDirectCommand(uint player, VMNetCommandBodyAbstract command) { throw new InvalidOperationException("Unexpected direct command"); }
    public override string GetUserIP(uint player) { throw new InvalidOperationException("No network accounts in this cohort"); }
}

internal static class OriginalVmBootstrap
{
    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
    private static string Digest(byte[] bytes)
    {
        using (var hash = SHA256.Create()) return BitConverter.ToString(hash.ComputeHash(bytes)).Replace("-", "").ToLowerInvariant();
    }
    private static void SetPrivate(Type type, object owner, string name, object value)
    {
        var field = type.GetField(name, BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static);
        Require(field != null, "Original content seam changed: " + name);
        field.SetValue(owner, value);
    }
    private static Content ConfigureFixtureContent()
    {
        // Explicit content-provider injection, NOT Content.Init or proof of
        // installed global/patch/tuning data. All VM constructors run unchanged.
        var content = (Content)FormatterServices.GetUninitializedObject(typeof(Content));
        content.TS1 = true;
        content.BasePath = Directory.GetCurrentDirectory();
        var iff = new IffFile(); iff.MarkThrowaway();
        var globals = new WorldGlobalProvider(content);
        SetPrivate(typeof(WorldGlobalProvider), globals, "Cache", new Dictionary<string, GameGlobal> {
            { "global", new GameGlobal { Resource = new GameGlobalResource(iff, null) } }
        });
        content.WorldObjectGlobals = globals;
        SetPrivate(typeof(Content), null, "INSTANCE", content);
        return content;
    }
    public static int Main(string[] args)
    {
        try
        {
            Require(args.Length == 1, "Supply the pinned original source checkout");
            Require(Environment.GetEnvironmentVariable("WONDERLAND_ORIGINAL_VM_TEST_ONLY") == "1", "Explicit test-only opt-in required");
            CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
            CultureInfo.DefaultThreadCurrentUICulture = CultureInfo.InvariantCulture;
            VM.UseWorld = false;
            VMContext.InitVMConfig(true);
            VMContext.BindAssembler();
            var content = ConfigureFixtureContent();
            var context = new VMContext(null);
            var driver = new FixtureDriver();
            var vm = new VM(context, driver, null);
            vm.Init();
            context.Architecture = new VMArchitecture(8, 8, null, context);
            context.Clock.UTCStart = 630822816000000000L;
            context.RandomSeed = 0x123456789abcdef0UL;
            Require(vm.GetType() == typeof(VM) && typeof(VM).Assembly == typeof(VMThread).Assembly, "Original simulation assembly identity required");
            Require(vm.GlobalLink == null && vm.EODHost == null, "No production effect provider allowed");
            Require(vm.GlobalState.Length == 38 && vm.GetGlobalValue(20) == 255, "Original VM.Init was not observed");
            var path = Path.Combine(args[0], "TSOClient/FSO.Content.TSO/Content/Objects/Casino_2-Tile_Bar_CC.iff");
            Require(Digest(File.ReadAllBytes(path)) == "20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865", "Original BHAV resource hash mismatch");
            var original = new IffFile(path); original.MarkThrowaway();
            var routine = original.Get<BHAV>(4110);
            Require(routine != null && routine.Instructions.Length > 0, "Original behavior must be decoded");
            var local = new IffFile(); local.MarkThrowaway(); local.AddChunk(routine);
            var definition = new OBJD {
                GUID = 0xf00d0044, ObjectType = OBJDType.Normal,
                NumAttributes = 4, StackSize = 8, BHAV_Init = 4110, BHAV_MainID = 4110,
                ChunkLabel = "Declared source-behavior witness", ChunkProcessed = true
            };
            var resource = new GameObjectResource(local, null, null, "swarm-f-original-4110", content);
            var entity = new VMGameObject(new GameObject { GUID = definition.GUID, OBJ = definition, Resource = resource }, null);
            for (int i = 0; i < 4; i++) entity.SetAttribute(i, 0);
            entity.MultitileGroup = new VMMultitileGroup(); entity.MultitileGroup.AddObject(entity);
            vm.AddEntity(entity); entity.Init(context);
            Require(entity.Thread != null && entity.Thread.GetType() == typeof(VMThread), "Real original interpreter thread required");
            Require(entity.GetAttribute(2) == 30, "Original init BHAV did not execute");
            Console.WriteLine("tick\tobject_id\tattribute_2\tstack_count\tclock_ticks\trng\tentities");
            for (int tick = 1; tick <= 16; tick++)
            {
                entity.SetAttribute(2, (short)-tick); vm.Tick();
                Require(vm.Ready && context.Clock.Ticks == tick, "Original VM driver/clock did not advance");
                Require(entity.GetAttribute(2) == 30 && !entity.Dead, "Original scheduled BHAV did not restore its literal value");
                Require(entity.Thread.Stack.Count == 0 && driver.LastTick == tick, "Original frame return/tick completion mismatch");
                Console.WriteLine(tick + "\t" + entity.ObjectID + "\t" + entity.GetAttribute(2) + "\t" + entity.Thread.Stack.Count + "\t" + context.Clock.Ticks + "\t" + context.RandomSeed + "\t" + vm.Entities.Count);
            }
            return 0;
        }
        catch (Exception error) { Console.Error.WriteLine(error.ToString()); return 1; }
    }
}
