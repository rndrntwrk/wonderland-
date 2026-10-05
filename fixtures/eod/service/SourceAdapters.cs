// Test-only VM, content and asynchronous provider boundaries. The original
// handler/codec files in sources.json compile unchanged. These callbacks expose
// authored responses and record effects; they do not implement a database/lot.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using FSO.SimAntics.NetPlay.EODs;
using FSO.SimAntics.NetPlay.EODs.Handlers;
using FSO.SimAntics.NetPlay.EODs.Model;
using FSO.SimAntics.NetPlay.Model;
namespace FSO.Common.Serialization {
 public interface IoBufferSerializable {}
 public class UnusedBuffer { public byte[] GetBytes(){throw new NotSupportedException("IoBuffer is outside service source oracle");} }
 public static class IoBufferUtils { public static UnusedBuffer SerializableToIoBuffer(IoBufferSerializable body,object context){throw new NotSupportedException("IoBuffer is outside service source oracle");} }
}
namespace FSO.SimAntics.Model.TSOPlatform {
 public enum VMTSOAvatarPermissions:byte {Visitor=0,Roommate=1,BuildBuyRoommate=2,Owner=3,Admin=4}
 public class VMTSOObjectState {public uint OwnerID=1;}
 public class AvatarState {public VMTSOAvatarPermissions Permissions;}
}
namespace FSO.SimAntics.Model {public enum VMPersonDataVariable {Gender}}
namespace FSO.SimAntics.Engine.Scopes {public enum VMPersonSuits {DynamicCostume=25}}
namespace FSO.SimAntics.Engine.TSOTransaction {public delegate void VMAsyncAccountUserIDFromAvatarCallback(uint userID);}
namespace FSO.Vitaboy {public class Collection:List<CollectionItem>{} public class CollectionItem{public ulong PurchasableOutfitId;} public class Purchasable{public ulong OutfitID;}}
namespace FSO.Content {
 public struct CatalogItem{public int DisableLevel;}
 public class WorldCatalog {public HashSet<uint> GetUntradableGUIDs(){return new HashSet<uint>{0xBAD};}public CatalogItem? GetItemByGUID(uint guid){return new CatalogItem{DisableLevel=guid==0xBAD?2:0};}}
 public class Collections {public FSO.Vitaboy.Collection Get(string path){return new FSO.Vitaboy.Collection{new FSO.Vitaboy.CollectionItem{PurchasableOutfitId=10}};}}
 public class Purchasables {public FSO.Vitaboy.Purchasable Get(ulong id){if(id!=10)throw new ArgumentException();return new FSO.Vitaboy.Purchasable{OutfitID=123};}}
 public class Content {static Content Singleton=new Content();public static Content Get(){return Singleton;}public WorldCatalog WorldCatalog=new WorldCatalog();public Collections AvatarCollections=new Collections();public Purchasables AvatarPurchasables=new Purchasables();}
}
namespace FSO.SimAntics {
 public class ThreadState {public short[] TempRegisters=new short[16];}
 public class Definition {public uint GUID=0x1234;}
 public class Resource {public Definition OBJ=new Definition();}
 public class VMEntity {public short ObjectID;public uint PersistID;public ThreadState Thread=new ThreadState();public Resource Object=new Resource();public object TSOState=new Model.TSOPlatform.VMTSOObjectState();}
 public class VMAvatar:VMEntity {public short Gender;public Model.TSOPlatform.AvatarState AvatarState=new Model.TSOPlatform.AvatarState();public short GetPersonData(Model.VMPersonDataVariable field){return Gender;}}
 public class Clock{public DateTime UTCNow=new DateTime(100000000000L);}
 public class Context{public Clock Clock=new Clock();}
 public class Lot{public bool CommunityLot;}
 public class VM {public Context Context=new Context();public Lot TSOState=new Lot();public Provider GlobalLink=new Provider();public List<object> Commands=new List<object>();public List<VMEntity> Entities=new List<VMEntity>();public VMEntity GetObjectById(short id){return Entities.FirstOrDefault(e=>e.ObjectID==id);}public void SendCommand(object cmd){Commands.Add(cmd);}}
 public class Inventory {public uint GUID;public byte[] Data;}
 public class Provider {
  public byte[] Data;public List<byte[]> Writes=new List<byte[]>();public bool Deferred;public List<Action<byte[]>> Loads=new List<Action<byte[]>>();public int BulletinQueries;public bool? CooldownAllowed=true;public DateTime Expires=new DateTime(100650000000L);public string Scope;public int Trades;public byte[] Newspaper;
  public void LoadPluginPersist(VM vm,uint obj,uint plugin,Action<byte[]> cb){if(Deferred)Loads.Add(cb);else cb(Data);}
  public void Release(){var callbacks=Loads.ToArray();Loads.Clear();foreach(var cb in callbacks)cb(Data);}
  public void SavePluginPersist(VM vm,uint obj,uint plugin,byte[] bytes){Data=(byte[])bytes.Clone();Writes.Add(Data);}
  public void GetDynPayouts(Action<byte[]> cb){cb(Newspaper);}
  public void GetBulletinState(VM vm,Action<uint,uint> cb){BulletinQueries++;cb(0x87654321,65537);}
  public void GetAccountIDFromAvatar(uint avatar,Engine.TSOTransaction.VMAsyncAccountUserIDFromAvatarCallback cb){cb(10);}
  public void GetObjectGlobalCooldown(VM vm,uint guid,uint avatar,uint account,TimeSpan duration,bool byAccount,bool category,Action<bool?,DateTime> cb){Scope=(byAccount?"account":"avatar")+"/"+(category?"category":"global");cb(CooldownAllowed,Expires);}
  public void RetrieveFromInventory(VM vm,uint item,uint avatar,bool take,Action<Inventory> cb){cb(new Inventory{GUID=item==99?0xBADu:0xACEu,Data=new byte[]{1,2,3}});}
  public void PerformTransaction(VM vm,bool test,uint from,uint to,int amount,Action<bool,int,uint,uint,uint,uint> cb){if(!test)throw new NotSupportedException("source oracle cannot transfer money");cb(true,amount,from,1000,to,1000);}
  public void FindLotAndValue(VM vm,uint avatar,HashSet<uint> untradable,Action<uint,int,long,string> cb){cb(55,2,123,"Home");}
  public void SecureTrade(VM vm,VMEODSecureTradePlayer a,VMEODSecureTradePlayer b,HashSet<uint> untradable,Action<VMEODSecureTradeError> cb){Trades++;cb(VMEODSecureTradeError.SUCCESS);}
 }
}
namespace FSO.SimAntics.NetPlay.Model.Commands {public class VMNetSetOutfitCmd {public uint UID;public Engine.Scopes.VMPersonSuits Scope;public ulong Outfit;}}
namespace FSO.SimAntics.NetPlay.EODs {
 public class VMEODServer {public VM vm;public VMEntity Object;public uint PluginID;public bool CanBeActionCancelled;public List<VMEODClient> Clients=new List<VMEODClient>();public VMEODHandler Handler;public VMEODServer(VM machine,uint plugin){vm=machine;PluginID=plugin;Object=new VMEntity{ObjectID=10,PersistID=10};}public void Disconnect(VMEODClient c){if(Clients.Remove(c)&&Handler!=null)Handler.OnDisconnection(c);}public void Shutdown(){foreach(var c in Clients.ToArray())Disconnect(c);}}
 public class VMEODClient {public VM vm;public VMAvatar Avatar;public VMEntity Invoker;public List<string> Ui=new List<string>();public List<VMEODEvent> Events=new List<VMEODEvent>();public void Send(string e,string b){Ui.Add(e+":t:"+b);}public void Send(string e,byte[] b){Ui.Add(e+":b:"+(b==null?"null":BitConverter.ToString(b).Replace("-","").ToLowerInvariant()));}public void Send(string e,VMSerializable b){using(var m=new MemoryStream()){var w=new BinaryWriter(m);b.SerializeInto(w);Send(e,m.ToArray());}}public void SendOBJEvent(VMEODEvent e){Events.Add(e);}}
}
namespace FSO.SimAntics.NetPlay.EODs.Handlers {public enum VMEODSignsMode {Read=0}}
