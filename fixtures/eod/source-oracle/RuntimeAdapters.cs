// Explicit test-only adapters for dependencies outside the original handler
// component. No original handler, host, server, command or persistence code is
// copied here. Unused registered handlers and renderer serializers fail loudly.
using System;
using System.Collections.Generic;
using System.Text;
using FSO.SimAntics.NetPlay.EODs;
using FSO.SimAntics.NetPlay.EODs.Model;
using FSO.SimAntics.NetPlay.Model;
using FSO.SimAntics.NetPlay.Model.Commands;

namespace FSO.SimAntics.Model.TSOPlatform
{
    // Numeric adapter values: original VMTSOAvatarState.cs:96-103.
    public enum VMTSOAvatarPermissions : byte
    { Visitor = 0, Roommate = 1, BuildBuyRoommate = 2, Owner = 3, Admin = 4 }
}

namespace FSO.SimAntics
{
    public sealed class OracleConnection
    {
        public readonly List<VMEODEvent> Events = new List<VMEODEvent>();
        public bool Ended;
    }
    public sealed class OracleThread
    {
        public short[] TempRegisters = new short[8];
        public OracleConnection EODConnection;
    }
    public class VMEntity
    {
        public short ObjectID;
        public uint PersistID;
        public readonly OracleThread Thread = new OracleThread();
    }
    public sealed class OracleAvatarState
    {
        public Model.TSOPlatform.VMTSOAvatarPermissions Permissions;
    }
    public sealed class VMAvatar : VMEntity
    {
        public readonly OracleAvatarState AvatarState = new OracleAvatarState();
    }
    public sealed class VM
    {
        public readonly Dictionary<short, VMEntity> Objects = new Dictionary<short, VMEntity>();
        public readonly VMEODHost EODHost = new VMEODHost();
        public readonly EodOracle.DeferredGlobalLink GlobalLink = new EodOracle.DeferredGlobalLink();
        public uint MyUID;
        public VMEntity GetObjectById(short id)
        {
            VMEntity value;
            return Objects.TryGetValue(id, out value) ? value : null;
        }
        public void SendCommand(VMNetCommandBodyAbstract command)
        {
            var evt = command as VMNetEODEventCmd;
            if (evt == null) throw new NotSupportedException("non-EOD synchronized command");
            EodOracle.Trace.Vm(evt.ObjectID, evt.Event.Code, evt.Event.Data);
            if (!evt.Execute(this)) throw new InvalidOperationException("source event had no live thread");
        }
        public void ForwardCommand(VMNetCommandBodyAbstract command)
        {
            var message = command as VMNetEODMessageCmd;
            if (message == null) throw new NotSupportedException("non-EOD private message");
            EodOracle.Trace.Ui(message.ActorUID, message.PluginID, message.Binary,
                message.EventName, message.Binary ? message.BinData : Encoding.UTF8.GetBytes(message.TextData));
        }
        public void SignalEODMessage(VMNetEODMessageCmd command)
        {
            throw new NotSupportedException("application UI is not part of this oracle");
        }
        public void CompleteEventBoundary()
        {
            // The fixture's VM adapter consumes emitted command events here.
            // It does not execute BHAVs or derive register changes from them.
            // Ended connections are cleared after all stage effects have been
            // observed, matching VMInvokePlugin.cs:39-57's completed boundary.
            foreach (var obj in Objects.Values)
            {
                var connection = obj.Thread.EODConnection;
                if (connection == null) continue;
                connection.Events.Clear();
                if (connection.Ended) obj.Thread.EODConnection = null;
            }
        }
    }
}

namespace EodOracle
{
    public sealed class DeferredGlobalLink
    {
        private sealed class Pending
        {
            public uint Object, Plugin;
            public byte[] Snapshot;
            public Action<byte[]> Callback;
        }
        private readonly Dictionary<Tuple<uint, uint>, byte[]> Data = new Dictionary<Tuple<uint, uint>, byte[]>();
        private readonly List<Pending> Loads = new List<Pending>();
        public void Seed(uint obj, uint plugin, byte[] data)
        {
            Data[Tuple.Create(obj, plugin)] = (byte[])data.Clone();
        }
        public void LoadPluginPersist(FSO.SimAntics.VM vm, uint obj, uint plugin, Action<byte[]> callback)
        {
            if (Loads.Count >= 128) throw new InvalidOperationException("pending load limit");
            byte[] bytes;
            Data.TryGetValue(Tuple.Create(obj, plugin), out bytes);
            Loads.Add(new Pending { Object = obj, Plugin = plugin,
                Snapshot = bytes == null ? null : (byte[])bytes.Clone(), Callback = callback });
            Trace.Provider("request", obj, plugin, null, false);
        }
        public void SavePluginPersist(FSO.SimAntics.VM vm, uint obj, uint plugin, byte[] data)
        {
            if (obj == 0) throw new InvalidOperationException("fixture cannot save a transient object");
            Trace.Provider("save", obj, plugin, data, true);
            Data[Tuple.Create(obj, plugin)] = (byte[])data.Clone();
        }
        public void Release()
        {
            if (Loads.Count == 0) throw new InvalidOperationException("no deferred load to release");
            var ready = Loads.ToArray();
            Loads.Clear();
            foreach (var pending in ready)
            {
                Trace.Provider("result", pending.Object, pending.Plugin, pending.Snapshot, pending.Snapshot != null);
                pending.Callback(pending.Snapshot);
            }
        }
    }
}

namespace FSO.Common.Serialization
{
    // EODLobby's IoBuffer overload is compiled but is never used by these five
    // handlers. Fail instead of providing a fake serializer if scope expands.
    public interface IoBufferSerializable { }
    public sealed class OracleUnusedBuffer
    {
        public byte[] GetBytes() { throw new NotSupportedException("unused IoBuffer serializer"); }
    }
    public static class IoBufferUtils
    {
        public static OracleUnusedBuffer SerializableToIoBuffer(IoBufferSerializable body, object context)
        { throw new NotSupportedException("unused IoBuffer serializer"); }
    }
}

namespace FSO.SimAntics.NetPlay.EODs.Handlers
{
    public abstract class OracleUnsupportedHandler : VMEODHandler
    {
        protected OracleUnsupportedHandler(VMEODServer server) : base(server)
        { throw new NotSupportedException("handler outside five-handler oracle scope"); }
    }
    public sealed class VMEODPizzaMakerPlugin : OracleUnsupportedHandler { public VMEODPizzaMakerPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODPaperChasePlugin : OracleUnsupportedHandler { public VMEODPaperChasePlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODRackOwnerPlugin : OracleUnsupportedHandler { public VMEODRackOwnerPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODRackPlugin : OracleUnsupportedHandler { public VMEODRackPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODDresserPlugin : OracleUnsupportedHandler { public VMEODDresserPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODSlotsPlugin : OracleUnsupportedHandler { public VMEODSlotsPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODTrunkPlugin : OracleUnsupportedHandler { public VMEODTrunkPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODWarGamePlugin : OracleUnsupportedHandler { public VMEODWarGamePlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODGameCompDrawACardPlugin : OracleUnsupportedHandler { public VMEODGameCompDrawACardPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODBandPlugin : OracleUnsupportedHandler { public VMEODBandPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODRoulettePlugin : OracleUnsupportedHandler { public VMEODRoulettePlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODSecureTradePlugin : OracleUnsupportedHandler { public VMEODSecureTradePlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODBlackjackPlugin : OracleUnsupportedHandler { public VMEODBlackjackPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODTwoPersonJobObjectMazePlugin : OracleUnsupportedHandler { public VMEODTwoPersonJobObjectMazePlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODNCDanceFloorPlugin : OracleUnsupportedHandler { public VMEODNCDanceFloorPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODDancePlatformPlugin : OracleUnsupportedHandler { public VMEODDancePlatformPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODDJStationPlugin : OracleUnsupportedHandler { public VMEODDJStationPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODNightclubControllerPlugin : OracleUnsupportedHandler { public VMEODNightclubControllerPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODFNewspaperPlugin : OracleUnsupportedHandler { public VMEODFNewspaperPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODHoldEmCasinoPlugin : OracleUnsupportedHandler { public VMEODHoldEmCasinoPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODBulletinPlugin : OracleUnsupportedHandler { public VMEODBulletinPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODCooldownEventPlugin : OracleUnsupportedHandler { public VMEODCooldownEventPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODGameshowBuzzerPlayerPlugin : OracleUnsupportedHandler { public VMEODGameshowBuzzerPlayerPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODGameshowBuzzerHostPlugin : OracleUnsupportedHandler { public VMEODGameshowBuzzerHostPlugin(VMEODServer s) : base(s) { } }
    public sealed class VMEODPropertySelectPlugin : OracleUnsupportedHandler { public VMEODPropertySelectPlugin(VMEODServer s) : base(s) { } }
}
