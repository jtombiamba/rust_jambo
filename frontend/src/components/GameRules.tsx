interface Props {
  isOpen: boolean
  onClose: () => void
}

export default function GameRules({ isOpen, onClose }: Props) {
  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="relative w-full max-w-lg max-h-[85vh] overflow-y-auto bg-white rounded-xl shadow-2xl mx-4">
        <button
          onClick={onClose}
          className="absolute top-3 right-3 w-8 h-8 flex items-center justify-center rounded-full bg-gray-100 hover:bg-gray-200 text-gray-500 text-lg font-bold"
          aria-label="Close rules"
        >
          &times;
        </button>

        <div className="p-6">
          <h2 className="text-2xl font-bold mb-6">How to Play Jambo</h2>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">Objective</h3>
            <p className="text-gray-700">
              Win rounds by playing the highest card of the leading suit. After 5 rounds,
              the player with the most credits wins the game.
            </p>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">The Deck</h3>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>32 cards total</li>
              <li>4 suits: <span className="text-red-600">Hearts</span>, <span className="text-black">Spades</span>, <span className="text-red-600">Diamonds</span>, <span className="text-black">Clubs</span></li>
              <li>Ranks: 3, 4, 5, 6, 7, 8, 9, 10</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">Setup</h3>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>4 players per game (you + 3 bots)</li>
              <li>5 cards dealt to each player</li>
              <li>10 credits bet per game</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">Turn Flow</h3>
            <ol className="list-decimal pl-5 space-y-1 text-gray-700">
              <li>The first player chooses any card from their hand and leads the round.</li>
              <li>Going clockwise, each other player must play a card of the <strong>same suit</strong> if they have one.</li>
              <li>If a player cannot follow suit, they may play any card.</li>
              <li>Once all 4 players have played, the round ends.</li>
            </ol>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">Winning a Round</h3>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>The highest card of the <strong>leading suit</strong> wins the round.</li>
              <li>Cards that do not follow the leading suit cannot win.</li>
              <li>The winner collects 30 credits (3 opponents &times; 10 bet).</li>
              <li>Each loser pays 10 credits.</li>
              <li>The round winner leads the next round.</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">Kora (Double Bet)</h3>
            <p className="text-gray-700 mb-1">
              If the winning card of a round is a <strong>3</strong> (the lowest rank), it triggers
              a <span className="text-yellow-600 font-semibold">Kora</span>:
            </p>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>The bet is doubled to 20 credits per player.</li>
              <li>The winner collects 60 credits (3 &times; 20).</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">Double Kora (Quadruple Bet)</h3>
            <p className="text-gray-700 mb-1">
              If the winning card is a <strong>3</strong> AND <strong>no one</strong> followed the
              leading suit, it triggers a <span className="text-yellow-600 font-semibold">Double Kora</span>:
            </p>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>The bet is quadrupled to 40 credits per player.</li>
              <li>The winner collects 120 credits (3 &times; 40).</li>
            </ul>
          </section>

          <section className="mb-5">
            <h3 className="text-lg font-semibold mb-2">Game End</h3>
            <ul className="list-disc pl-5 space-y-1 text-gray-700">
              <li>The game ends after all 5 rounds are played.</li>
              <li>The player with the most credits at the end wins the game.</li>
              <li>If the final round is a Kora or Double Kora, the game status reflects that.</li>
            </ul>
          </section>

          <button
            onClick={onClose}
            className="w-full mt-4 px-6 py-3 bg-blue-600 text-white font-semibold rounded-lg hover:bg-blue-700"
          >
            Got it!
          </button>
        </div>
      </div>
    </div>
  )
}
