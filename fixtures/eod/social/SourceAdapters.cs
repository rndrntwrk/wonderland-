// Test-only VM/transport adapters. Original handlers are compiled unchanged.
// These adapters record requested effects; they do not execute a simulated lot.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using FSO.SimAntics.Model;
using FSO.SimAntics.NetPlay.EODs;
using FSO.SimAntics.NetPlay.EODs.Model;

namespace FSO.SimAntics.NetPlay.Model {
    public interface VMSerializable { void SerializeInto(BinaryWriter writer); void Deserialize(BinaryReader reader); }
}
namespace FSO.Common.Serialization {
    public interface IoBufferSerializable { }
    public sealed class UnusedBuffer { public byte[] GetBytes() { throw new NotSupportedException("unused oracle path"); } }
    public static class IoBufferUtils { public static UnusedBuffer SerializableToIoBuffer(IoBufferSerializable body, object context) { throw new NotSupportedException("unused oracle path"); } }
}
namespace FSO.SimAntics {
    public sealed class Position { public int TileX; public int TileY; }
    public sealed class ObjectResource { public string Name = "oracle"; }
    public sealed class ObjectDefinition { public ObjectResource Resource = new ObjectResource(); }
    public sealed class QueueItem { public VMEntity Callee; public ushort InteractionNumber; public ushort UID; }
    public sealed class ThreadState { public short[] TempRegisters = new short[16]; public List<QueueItem> Queue = new List<QueueItem>(); public int ActiveQueueBlock = -1; }
    public class VMEntity {
        public short ObjectID;
        public uint Guid;
        public short GroupID;
        public Position Position = new Position();
        public ObjectDefinition Object = new ObjectDefinition();
        public ThreadState Thread = new ThreadState();
        public short GetValue(VMStackObjectVariable field) { if (field != VMStackObjectVariable.GroupID) throw new NotSupportedException("unexpected variable"); return GroupID; }
    }
    public sealed class VMAvatar : VMEntity {
        public string Name;
        public short Charisma; public short Body; public short Creativity;
        public short GetPersonData(VMPersonDataVariable field) {
            switch (field) {
                case VMPersonDataVariable.CharismaSkill: return Charisma;
                case VMPersonDataVariable.BodySkill: return Body;
                case VMPersonDataVariable.CreativitySkill: return Creativity;
                default: throw new NotSupportedException("unexpected skill");
            }
        }
    }
    public sealed class Queries {
        public List<VMAvatar> Avatars = new List<VMAvatar>();
        public List<VMEntity> Objects = new List<VMEntity>();
        public List<VMEntity> GetObjectsByGUID(uint guid) { return Objects.Where(item => item.Guid == guid).ToList(); }
    }
    public sealed class Context { public Queries ObjectQueries = new Queries(); }
    public sealed class HandlerList {
        public List<VMEODHandler> Handlers = new List<VMEODHandler>();
        public T GetFirstHandler<T>() where T : VMEODHandler { return Handlers.OfType<T>().FirstOrDefault(); }
        public IEnumerable<T> GetHandlers<T>() where T : VMEODHandler { return Handlers.OfType<T>(); }
    }
    public sealed class VM {
        public List<VMEntity> Entities = new List<VMEntity>();
        public Context Context = new Context();
        public HandlerList EODHost = new HandlerList();
        public List<object> Commands = new List<object>();
        public void SendCommand(object command) { Commands.Add(command); }
        public void ForwardCommand(object command) { Commands.Add(command); }
        public VMEntity GetObjectById(short id) { return Entities.FirstOrDefault(entity => entity.ObjectID == id); }
    }
}
namespace FSO.SimAntics.NetPlay.Model.Commands {
    public sealed class VMNetBatchGraphicCmd { public short[] Objects; public byte[] Graphics; }
    public sealed class VMNetInteractionCmd { public short CalleeID; public short CallerID; public bool Global; public ushort Interaction; public bool FromNet; }
}
namespace FSO.SimAntics.NetPlay.EODs {
    public sealed class VMEODServer {
        public FSO.SimAntics.VM vm;
        public List<VMEODClient> Disconnected = new List<VMEODClient>();
        public VMEODServer(FSO.SimAntics.VM machine) { vm = machine; }
        public void Disconnect(VMEODClient client) { Disconnected.Add(client); }
    }
    public class VMEODHandler {
        protected VMEODServer Server;
        public Dictionary<string, Action<string, string, VMEODClient>> PlaintextHandlers = new Dictionary<string, Action<string, string, VMEODClient>>();
        public Dictionary<string, Action<string, byte[], VMEODClient>> BinaryHandlers = new Dictionary<string, Action<string, byte[], VMEODClient>>();
        public Dictionary<short, Action<short, VMEODClient>> SimanticsHandlers = new Dictionary<short, Action<short, VMEODClient>>();
        public VMEODHandler(VMEODServer server) { Server = server; }
        public virtual void OnConnection(VMEODClient client) { }
        public virtual void OnDisconnection(VMEODClient client) { }
        public virtual void Tick() { }
        public virtual void SelfResync() { }
    }
    public sealed class VMEODClient {
        public FSO.SimAntics.VMAvatar Avatar;
        public FSO.SimAntics.VMEntity Invoker;
        public List<string> Ui = new List<string>();
        public List<VMEODEvent> Events = new List<VMEODEvent>();
        public void Send(string name, string text) { Ui.Add(name + ":t:" + text); }
        public void Send(string name, byte[] bytes) { Ui.Add(name + ":b:" + BitConverter.ToString(bytes).Replace("-", "").ToLowerInvariant()); }
        public void SendOBJEvent(VMEODEvent effect) { Events.Add(effect); }
    }
}
