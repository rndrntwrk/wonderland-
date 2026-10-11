// Drives unchanged original components through the shared bounded trace protocol.
// All inputs are authored fixtures. The application VM and UI are not executed.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using FSO.SimAntics;
using FSO.SimAntics.Model.TSOPlatform;
using FSO.SimAntics.NetPlay.Model.Commands;

namespace EodOracle
{
    public static class Trace
    {
        private static long Bytes;
        private static int Rows;
        public static void Row(params string[] fields)
        {
            string row = String.Join("\t", fields);
            if (row.Length > 300 * 1024 || ++Rows > 100000 || (Bytes += row.Length + 1) > 16 * 1024 * 1024)
                throw new InvalidOperationException("trace bound exceeded");
            Console.WriteLine(row);
        }
        public static string Hex(byte[] bytes)
        {
            if (bytes.Length == 0) return "-";
            if (bytes.Length > 128 * 1024) throw new InvalidOperationException("trace payload limit");
            return BitConverter.ToString(bytes).Replace("-", "").ToLowerInvariant();
        }
        public static void Vm(short invoker, short code, short[] temps)
        {
            Row("VM", invoker.ToString(), code.ToString(), temps.Length == 0 ? "-" : String.Join(",", temps));
        }
        public static void Ui(uint actor, uint plugin, bool binary, string evt, byte[] bytes)
        {
            Row("UI", actor.ToString(), plugin.ToString("x8"), binary ? "binary" : "text", evt, Hex(bytes));
        }
        public static void Provider(string kind, uint obj, uint plugin, byte[] bytes, bool exists)
        {
            if (kind == "request") Row("PROVIDER", kind, obj.ToString(), plugin.ToString("x8"));
            else if (kind == "result") Row("PROVIDER", kind, obj.ToString(), plugin.ToString("x8"), exists ? "1" : "0", exists ? Hex(bytes) : "-");
            else if (kind == "save") Row("PROVIDER", kind, obj.ToString(), plugin.ToString("x8"), Hex(bytes));
            else throw new ArgumentException("provider kind");
        }
    }

    public sealed class Runtime
    {
        private sealed class Binding
        {
            public VMEntity Invoker;
            public VMAvatar Avatar;
            public uint Plugin;
        }
        public readonly VM Vm = new VM();
        public readonly QueuedScheduler Scheduler = new QueuedScheduler();
        private readonly Dictionary<string, Binding> Bindings = new Dictionary<string, Binding>();

        private static uint Plugin(string value)
        {
            switch (value)
            {
                case "timer": return 0xaa65fe9e;
                case "dance": return 0x4a5be8ab;
                case "signs": return 0x2a6356a0;
                case "scoreboard": return 0x0949e698;
                case "door": return 0x0a69f29f;
                default: throw new ArgumentException("plugin outside oracle scope");
            }
        }
        private static short Id(string value)
        {
            short result = short.Parse(value, CultureInfo.InvariantCulture);
            if (result <= 0) throw new ArgumentException("fixture ObjectID must be positive Int16");
            return result;
        }
        public static byte[] Unhex(string value)
        {
            if (value == "-") return new byte[0];
            if (value.Length > 256 * 1024 || value.Length % 2 != 0) throw new ArgumentException("hex length");
            var result = new byte[value.Length / 2];
            for (int i = 0; i < result.Length; i++)
            {
                string digits = value.Substring(i * 2, 2);
                if (digits.Any(x => !((x >= '0' && x <= '9') || (x >= 'a' && x <= 'f'))))
                    throw new ArgumentException("canonical hex required");
                result[i] = byte.Parse(digits, NumberStyles.HexNumber, CultureInfo.InvariantCulture);
            }
            return result;
        }
        private static short[] Registers(string value)
        {
            var values = value.Split(',');
            if (values.Length < 4 || values.Length > 8) throw new ArgumentException("register count");
            var result = new short[8];
            for (int i = 0; i < values.Length; i++) result[i] = short.Parse(values[i], CultureInfo.InvariantCulture);
            return result;
        }
        private VMEntity Entity(short id, uint persistentId)
        {
            var entity = Vm.GetObjectById(id);
            if (entity != null)
            {
                if (entity.PersistID != persistentId) throw new ArgumentException("object identity changed");
                return entity;
            }
            entity = new VMEntity { ObjectID = id, PersistID = persistentId };
            Vm.Objects.Add(id, entity);
            return entity;
        }
        private void Connect(string[] fields)
        {
            var plugin = Plugin(fields[4]);
            var obj = Entity(Id(fields[5]), uint.Parse(fields[6], CultureInfo.InvariantCulture));
            var invoker = Entity(Id(fields[7]), fields[7] == fields[5] ? obj.PersistID : 0);
            var avatar = new VMAvatar { PersistID = uint.Parse(fields[8], CultureInfo.InvariantCulture), ObjectID = Id(fields[9]) };
            int permission = int.Parse(fields[10], CultureInfo.InvariantCulture);
            if (permission < 0 || permission > 4 || (fields[11] != "0" && fields[11] != "1"))
                throw new ArgumentException("authority fixture fields");
            avatar.AvatarState.Permissions = (VMTSOAvatarPermissions)permission;
            Vm.Objects[avatar.ObjectID] = avatar;
            invoker.Thread.TempRegisters = Registers(fields[12]);
            invoker.Thread.EODConnection = new OracleConnection();
            if (Bindings.Count >= 128) throw new InvalidOperationException("binding limit");
            Bindings.Add(fields[3], new Binding { Invoker = invoker, Avatar = avatar, Plugin = plugin });
            // Authorization is a trusted native invocation field. The original
            // handlers receive the exact mode/permission registers unchanged.
            Vm.EODHost.Connect(plugin, invoker, obj, avatar, plugin == 0x4a5be8ab, Vm);
        }
        public void Execute(string[] fields)
        {
            string op = fields[2];
            switch (op)
            {
                case "reset": return;
                case "seed":
                    Vm.GlobalLink.Seed(uint.Parse(fields[4], CultureInfo.InvariantCulture), Plugin(fields[3]), Unhex(fields[5]));
                    return;
                case "connect": Connect(fields); return;
                case "controller":
                    var obj = Entity(Id(fields[4]), 0);
                    var invoker = Entity(Id(fields[5]), 0);
                    invoker.Thread.EODConnection = new OracleConnection();
                    Bindings.Add(fields[3], new Binding { Invoker = invoker, Plugin = Plugin("dance") });
                    Vm.EODHost.Connect(Plugin("dance"), invoker, obj, null, true, Vm);
                    return;
                case "release":
                case "policy_release": Vm.GlobalLink.Release(); return;
                case "registers": Bindings[fields[3]].Invoker.Thread.TempRegisters = Registers(fields[4]); return;
                case "tick":
                    int count = int.Parse(fields[3], CultureInfo.InvariantCulture);
                    if (count < 1 || count > 10000) throw new ArgumentException("tick count");
                    for (int i = 0; i < count; i++) Vm.EODHost.Tick();
                    return;
                case "text":
                case "binary":
                case "policy_binary":
                    var binding = Bindings[fields[3]];
                    var payload = Unhex(fields[5]);
                    var binary = op != "text";
                    var message = new VMNetEODMessageCmd {
                        ActorUID = binding.Avatar.PersistID, PluginID = binding.Plugin,
                        Binary = binary, EventName = fields[4],
                        BinData = binary ? payload : null,
                        TextData = binary ? null : new UTF8Encoding(false, true).GetString(payload)
                    };
                    // Execute the real source routing/catch boundary; Verify
                    // forwards client input into EODHost and returns false.
                    if (message.Verify(Vm, binding.Avatar)) throw new InvalidOperationException("source forwarded inbound command");
                    return;
                case "disconnect": Vm.EODHost.ForceDisconnectObj(Bindings[fields[3]].Invoker); return;
                default: throw new ArgumentException("unknown operation");
            }
        }
    }

    public static class Program
    {
        private static readonly Dictionary<string, int> Arity = new Dictionary<string, int> {
            { "reset", 0 }, { "seed", 3 }, { "connect", 10 }, { "controller", 3 },
            { "release", 0 }, { "policy_release", 0 }, { "registers", 2 }, { "tick", 1 },
            { "text", 3 }, { "binary", 3 }, { "policy_binary", 3 }, { "disconnect", 1 }
        };
        private static string ReadInput()
        {
            using (var input = Console.OpenStandardInput())
            using (var result = new MemoryStream())
            {
                var block = new byte[8192];
                int count;
                while ((count = input.Read(block, 0, block.Length)) > 0)
                {
                    if (result.Length + count > 2 * 1024 * 1024) throw new ArgumentException("scenario byte limit");
                    if (block.Take(count).Any(x => x > 127 || x == 0)) throw new ArgumentException("ASCII scenario protocol required");
                    result.Write(block, 0, count);
                }
                return Encoding.ASCII.GetString(result.ToArray());
            }
        }
        public static int Main()
        {
            try
            {
                Thread.CurrentThread.CurrentCulture = CultureInfo.InvariantCulture;
                Thread.CurrentThread.CurrentUICulture = CultureInfo.InvariantCulture;
                string input = ReadInput();
                if (!input.EndsWith("\n")) throw new ArgumentException("terminated scenario lines required");
                Runtime runtime = null;
                int steps = 0;
                foreach (var line in input.Split('\n'))
                {
                    if (line.Length == 0) continue;
                    if (line.Length > 300 * 1024 || ++steps > 4096) throw new ArgumentException("scenario line/step limit");
                    var fields = line.Split('\t');
                    int arity;
                    if (fields.Length < 3 || !Arity.TryGetValue(fields[2], out arity) || fields.Length != arity + 3)
                        throw new ArgumentException("operation arity");
                    Trace.Row("BEGIN", fields[0], fields[1]);
                    if (fields[2] == "reset") runtime = new Runtime();
                    if (runtime == null) throw new ArgumentException("reset required");
                    try { runtime.Scheduler.Run(() => runtime.Execute(fields)); }
                    catch (Exception error)
                    {
                        if (!fields[2].StartsWith("policy_", StringComparison.Ordinal)) throw;
                        Trace.Row("ERROR", error.GetBaseException().GetType().Name);
                    }
                    runtime.Vm.CompleteEventBoundary();
                    Trace.Row("END", fields[0], fields[1]);
                }
                return 0;
            }
            catch (Exception error)
            {
                Console.Error.WriteLine(error);
                return 1;
            }
        }
    }
}
