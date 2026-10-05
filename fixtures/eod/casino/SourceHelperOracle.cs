// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
// Calls original helper implementations extracted verbatim by verify.py.
// No VM, transport, provider, UI or random-sequence parity is simulated here.
using System;
using System.Globalization;
using System.Linq;
using FSO.SimAntics.NetPlay.EODs.Handlers;
using FSO.SimAntics.NetPlay.EODs.Utils;

public static class SourceHelperOracle
{
    private static PlayingCard Card(int rank, int suit)
    {
        return new PlayingCard((PlayingCardValues)rank, (PlayingCardSuits)suit);
    }

    private static BlackjackPlayer Hand(int first, int second, int bet)
    {
        var player = new BlackjackPlayer();
        player.BetAmount = bet;
        player.DealFirstTwoCards(0, Card(first, 0), Card(second, 1));
        return player;
    }

    private static void Print(string key, object value)
    {
        Console.WriteLine(key + "=" + Convert.ToString(value, CultureInfo.InvariantCulture));
    }

    private static string State(BlackjackPlayer player)
    {
        return String.Format(CultureInfo.InvariantCulture, "{0},{1},{2}",
            player.CurrentHandTotal, player.SoftTotal, (byte)player.CurrentHandType);
    }

    private static int Payout(int first, int second, int bet, int dealerTotal,
        VMEODBlackjackHandTypes dealerType, bool insured)
    {
        var player = Hand(first, second, bet);
        player.IsInsured = insured;
        player.Stand();
        return player.CalculateTotalPayout(dealerTotal, dealerType);
    }

    private static int DoublePayout(int first, int second, int third,
        int dealerTotal, VMEODBlackjackHandTypes dealerType)
    {
        var player = Hand(first, second, 10);
        player.Double(Card(third, 2));
        return player.CalculateTotalPayout(dealerTotal, dealerType);
    }

    public static void Main()
    {
        // Random order is intentionally unobserved. These are deterministic
        // invariants of the unchanged original circular/discard implementations.
        var deck = new AbstractPlayingCardsDeck(1, false);
        var draws = deck.DrawStrings(104);
        Print("deck.circular", String.Format(CultureInfo.InvariantCulture, "{0},{1},{2}",
            draws.Length, draws.Distinct().Count(),
            Enumerable.Range(0, 52).All(i => draws[i] == draws[i + 52])));
        var six = new AbstractPlayingCardsDeck(6, false).DrawStrings(312);
        Print("deck.six", String.Format(CultureInfo.InvariantCulture, "{0},{1},{2}",
            six.Length, six.Distinct().Count(), six.GroupBy(card => card).All(g => g.Count() == 6)));
        var discard = new AbstractPlayingCardsDeck(1, true);
        var discarded = discard.DrawStrings(52);
        Print("deck.discard", discarded.Length + "," + (discard.Draw() == null));
        Print("cards.shorthand", AbstractPlayingCardsDeck.CardShortHand.Count + "," +
            AbstractPlayingCardsDeck.CardShortHand["Ace_Clubs"] + "," +
            AbstractPlayingCardsDeck.CardShortHand["King_Spades"]);
        var original = Card(1, 3);
        original.Next = Card(2, 0);
        var copied = new PlayingCard(original);
        Print("cards.copy", copied.Value + "_" + copied.Suit + "," + (copied.Next == null));
        Print("cards.names", String.Join(",", Hand(1, 13, 5).GetCurrentCards()));

        Print("hand.natural", State(Hand(1, 13, 5)));
        Print("hand.ten_value_split", State(Hand(13, 12, 10)));
        Print("hand.split_aces", State(Hand(1, 1, 10)));
        var soft = Hand(1, 6, 10);
        Print("hand.soft17", State(soft));
        soft.Hit(Card(10, 2));
        Print("hand.soft17_after_ten", State(soft));
        soft.Stand();
        Print("hand.stood17", State(soft));
        var aces = Hand(1, 1, 10);
        aces.Hit(Card(9, 2));
        Print("hand.multiple_aces", State(aces));
        var bust = Hand(10, 6, 10);
        bust.Hit(Card(13, 2));
        Print("hand.bust", State(bust));
        var doubled = Hand(5, 6, 10);
        doubled.Double(Card(13, 2));
        Print("hand.double", State(doubled));

        var stand = VMEODBlackjackHandTypes.Stand;
        var natural = VMEODBlackjackHandTypes.Blackjack;
        Print("payout.win", Payout(10, 9, 10, 18, stand, false));
        Print("payout.push", Payout(10, 9, 10, 19, stand, false));
        Print("payout.loss", Payout(10, 9, 10, 20, stand, false));
        Print("payout.dealer_bust", Payout(10, 9, 10, 22, VMEODBlackjackHandTypes.Bust, false));
        Print("payout.odd_natural", Payout(1, 13, 5, 20, stand, false));
        Print("payout.natural_beats_non_natural21", Payout(1, 13, 5, 21, stand, false));
        Print("payout.natural_push", Payout(1, 13, 5, 21, natural, false));
        Print("payout.dealer_natural", Payout(9, 8, 5, 21, natural, false));
        Print("payout.odd_insurance", Payout(9, 8, 5, 21, natural, true));
        Print("payout.one_unit_insurance", Payout(9, 8, 1, 21, natural, true));
        Print("payout.insurance_without_natural", Payout(9, 8, 5, 18, stand, true));
        Print("payout.double_win", DoublePayout(5, 6, 10, 20, stand));
        Print("payout.double_push", DoublePayout(5, 6, 10, 21, stand));
        Print("payout.double_loss", DoublePayout(2, 3, 10, 18, stand));
        Print("payout.double_bust", DoublePayout(9, 10, 5, 22, VMEODBlackjackHandTypes.Bust));

        var split = Hand(13, 12, 10);
        Print("split.cards", String.Join(",", split.Split(new[] { Card(1, 2), Card(6, 3) })));
        Print("split.natural", State(split));
        split.StandAndGotoNextHand();
        split.Double(Card(5, 2));
        Print("split.double", State(split));
        Print("split.payout", split.CalculateTotalPayout(18, stand));
        var splitAces = Hand(1, 1, 10);
        splitAces.Split(new[] { Card(13, 2), Card(9, 3) });
        splitAces.StandAndGotoNextHand();
        splitAces.Stand();
        Print("split.aces_payout", splitAces.CalculateTotalPayout(18, stand));
    }
}
