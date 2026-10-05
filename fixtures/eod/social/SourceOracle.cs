using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using FSO.SimAntics;
using FSO.SimAntics.NetPlay.EODs;
using FSO.SimAntics.NetPlay.EODs.Handlers;
using FSO.SimAntics.NetPlay.EODs.Handlers.Data;

public static class SourceOracle {
    static string Hex(byte[] value) { return BitConverter.ToString(value).Replace("-", "").ToLowerInvariant(); }
    static object Field(object value, string name) { return value.GetType().GetField(name, BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public).GetValue(value); }
    static void Set(object value, string name, object contents) { value.GetType().GetField(name, BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public).SetValue(value, contents); }
    static void Check(bool condition, string description) { if (!condition) throw new Exception(description); }
    static VMEODClient Controller(short id) { return new VMEODClient { Invoker = new VMEntity { ObjectID = id } }; }
    static VMEODClient Player(short id, short role) { var value = new VMEODClient { Avatar = new VMAvatar { ObjectID = id, Name = "Avatar " + id }, Invoker = new VMEntity { ObjectID = (short)(id + 1000) } }; value.Invoker.Thread.TempRegisters[0] = role; return value; }
    public static void Main() {
        var vm = new VM(); var server = new VMEODServer(vm);
        var war = new VMEODWarGamePlugin(server); var control = Controller(1); var blue = Player(101, 0); var red = Player(102, 1);
        war.OnConnection(control); war.OnConnection(blue); war.OnConnection(red);
        var table = new List<string>();
        for (int b = 0; b < 5; b++) for (int r = 0; r < 5; r++) {
            var bluePiece = new VMEODWarGamePiece((VMEODWarGamePieceTypes)b); bluePiece.AddChildren();
            var redPiece = new VMEODWarGamePiece((VMEODWarGamePieceTypes)r); redPiece.AddChildren();
            int loser = bluePiece.PieceType == redPiece.PieceType ? -1 : redPiece.Defeats.Any(p => p.PieceType == bluePiece.PieceType) ? 0 : bluePiece.Defeats.Any(p => p.PieceType == redPiece.PieceType) ? 1 : -1;
            table.Add(loser.ToString());
        }
        Console.WriteLine("war.pairings=" + string.Join(",", table));
        var players = (List<VMEODWarGamePlayerPieces>)Field(war, "Players");
        players[0].Pieces.RemoveAll(p => p.PieceType != VMEODWarGamePieceTypes.Artillery);
        players[1].Pieces.RemoveAll(p => p.PieceType != VMEODWarGamePieceTypes.Cavalry);
        blue.Ui.Clear(); red.Ui.Clear(); control.Events.Clear();
        war.SimanticsHandlers[10](10, control);
        Check(blue.Ui.Count == 0 && red.Ui.Count == 0 && control.Events.Count == 0, "source unequal final pieces must reproduce the stall");
        Console.WriteLine("war.unequal_final_pieces_source_outputs=0");

        var band = new VMEODBandPlugin(new VMEODServer(new VM()));
        var payout = (int[])Field(band, "PayoutScheme");
        Console.WriteLine("band.payouts=" + string.Join(",", new[] { 0, 1, 5, 10, 15, 20, 25 }.Select(i => payout[i])));
        var skillValues = new List<string>();
        foreach (var value in new[] { 0.25m, 0.75m }) skillValues.Add(((short)Math.Round(value * VMEODBandPlugin.SKILL_PAYOUT_MULTIPLIER)).ToString());
        Console.WriteLine("band.skill_midpoints=" + string.Join(",", skillValues));

        var fresh = new VMEODFreshnessTracker(1); fresh.SendCommand(2); fresh.SendCommand(2);
        Check(fresh.LastCommands[0].SequenceEqual(new[] { -1, -1, -1 }), "source misses retain no actual command");
        Console.WriteLine("freshness.repeated_command_history=" + string.Join(",", fresh.LastCommands[0]));
        for (int i = 0; i < 30; i++) fresh.Tick();
        Console.WriteLine("freshness.after_30_ticks_bits=" + Hex(BitConverter.GetBytes((float)Field(fresh, "InternalFreshness"))));

        var floorVM = new VM();
        for (int y = 0; y < 9; y++) for (int x = 0; x < 9; x++) floorVM.Context.ObjectQueries.Objects.Add(new VMEntity { ObjectID = (short)(1 + x + y * 9), Guid = 0xD481CEE5, Position = new Position { TileX = x, TileY = y } });
        var floor = new VMEODNCDanceFloorPlugin(new VMEODServer(floorVM)); var floorControl = Controller(900);
        floor.OnConnection(floorControl); floor.S_DiscoverTiles(1, floorControl);
        floorControl.Invoker.Thread.TempRegisters[0] = 1; floorControl.Invoker.Thread.TempRegisters[1] = 3; floor.S_SetAnimation(2, floorControl);
        floor.DrawAnimation(0); Console.WriteLine("floor.fill0=" + Hex(floor.ScreenData));
        floor.DrawAnimation(10); Console.WriteLine("floor.fill10=" + Hex(floor.ScreenData));
        floorControl.Invoker.Thread.TempRegisters[0] = 5; floor.S_SetAnimation(2, floorControl);
        floor.DrawAnimation(0); Console.WriteLine("floor.heart=" + Hex(floor.ScreenData));
        floor.Animation = VMEODNCAnimTypes.Random; floor.RandomAnimation = VMEODNCRandomAnims.RainbowTunnel;
        floor.DrawAnimation(1); Console.WriteLine("floor.rainbow1=" + Hex(floor.ScreenData));
        var glyphs = new List<byte>(); foreach (char character in "FSOpg~") for (int row = 0; row < 6; row++) glyphs.Add(VMEOD3x5Font.GetFontLine(character, row));
        Console.WriteLine("floor.glyph_rows=" + Hex(glyphs.ToArray()));

        var buzzerVM = new VM(); var host = new VMEODGameshowBuzzerHostPlugin(new VMEODServer(buzzerVM)); buzzerVM.EODHost.Handlers.Add(host);
        var hostControl = Controller(500); host.OnConnection(hostControl);
        var contestantPlugins = new List<VMEODGameshowBuzzerPlayerPlugin>(); var contestants = new List<VMEODClient>();
        for (int i = 0; i < 4; i++) {
            var plugin = new VMEODGameshowBuzzerPlayerPlugin(new VMEODServer(buzzerVM)); buzzerVM.EODHost.Handlers.Add(plugin);
            plugin.OnConnection(Controller((short)(501 + i))); var player = Player((short)(201 + i), 0); plugin.OnConnection(player); contestants.Add(player); contestantPlugins.Add(plugin);
        }
        var hostPlayer = Player(300, 0); host.OnConnection(hostPlayer);
        host.BinaryHandlers["Buzzer_Host_PlayerCorrect"]("Buzzer_Host_PlayerCorrect", BitConverter.GetBytes(0), hostPlayer);
        Console.WriteLine("buzzer.auto_enable=" + string.Join(",", contestantPlugins.Select(p => p.MyBuzzerEnabled ? "1" : "0")));
        Check(contestantPlugins.Select(p => p.MyBuzzerEnabled).SequenceEqual(new[] { true, true, true, false }), "original auto-enable loop covers first three only");
        Check(contestantPlugins[0].MyScore == 100, "source correct answer score");
        host.BinaryHandlers["Buzzer_Host_A_ToggleMaster"]("Buzzer_Host_A_ToggleMaster", new byte[] { 1 }, hostPlayer);
        contestantPlugins[0].BinaryHandlers["Buzzer_Player_Buzzed"]("Buzzer_Player_Buzzed", new byte[0], contestants[0]);
        for (int i = 0; i < 29; i++) host.Tick();
        Console.WriteLine("buzzer.state29=" + Field(host, "_BuzzerState"));
        host.Tick(); Console.WriteLine("buzzer.state30=" + Field(host, "_BuzzerState"));
        Console.WriteLine("source_oracle=PASS");
    }
}
