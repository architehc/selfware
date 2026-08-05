# Syndromes, detectors, and the time dimension

Tier 1 quietly assumed the referees never make mistakes: every whistle meant a real error, every silence meant a clean board. Real hardware is not like that. Measurements are physical operations, and they lie — a check can whistle when nothing happened, or sit silent through a hit. This lesson shows the fix, and it is one of the neatest ideas in the whole subject: stop trusting readings, and start trusting *changes*. The price is a third dimension — time — and the reward is that everything you learned about chains and endpoints survives intact, one floor up.

## Referees are noisy too

A superconducting-qubit measurement might report the wrong answer once in a hundred tries. That is a disaster for the single-round picture:

- A **false positive**: a check whistles, you see an anyon, but no error ever happened. Chase it and you may *create* the very chain you were trying to repair.
- A **false negative**: an error strikes, the responsible check misreads, and the chain's endpoint goes unreported. The damage is invisible.

One round of measurements, then, is weak evidence either way. A fired check might be a liar; a quiet check might be asleep on duty. If the code had only one round to work with, surface-code error correction would simply not work.

## Repeat everything in time

The way out is to never measure once. The quantum computer runs in **rounds** (also called cycles): in every round, every check is measured again, from cold start to quiet board, over and over for as long as the computation lasts. A surface-code memory is not a single chessboard — it is a movie of chessboards, thousands or millions of frames long.

Repetition alone does not remove the lies, but it makes them *identifiable*, because a lie and a truth behave differently over time. A real data-qubit error flips a check's value and the check *stays* flipped — the error persists on the qubit. A measurement lie flips the reported value for one round only, and the next round it flips back. Persistence versus blip: that is the signal.

## Detectors: trust only changes

The machine that extracts the signal is the **detector**, and it is the single most important definition of tier 2:

> A detector is the parity (odd/even comparison) of two consecutive measurements of the same check. It **fires** — produces a *detection event* — exactly when the check's answer changes from one round to the next.

Quiet followed by quiet: no event. Loud followed by loud: no event. Only a *flip*, in either direction, counts. Watch what this does to our two failure modes:

- A **data-qubit error** flips the check persistently, so the detector fires exactly once: at the round boundary where the error landed. (It fires at two checks, the ends of the error's chain — the spatial pairs you already know.)
- A **measurement error** flips the reading for one round and then it flips back, so the detector fires *twice*: once when the lie begins, once when it ends.

That last bullet deserves a pause. A lying referee produces a **pair of detection events separated in time**, at the same check, one round apart — exactly the way a data error produces a pair separated in space. The parity trick turns measurement noise into just another kind of error chain, and the pair rule from tier 1 is promoted to a law of the whole spacetime picture: *detection events come in pairs, or singly at a boundary.*

## The third dimension

Take the movie of chessboards and stack the frames. The result is a **3D spacetime graph** — two dimensions of space, one of time — and this graph, not the flat board, is the arena the rest of the lab lives in:

- A **data-qubit error** is a space-like edge: it connects two checks within one time slice, the endpoints of a chain segment on the board.
- A **measurement error** is a time-like edge: it connects a check to itself one round later.
- Real faults in the syndrome-extraction circuit — a bad gate spraying errors onto several qubits — become short diagonal edges mixing space and time.

An error chain in this world is a path through the stack, wandering in space and climbing in time, and the syndrome — all the referee information the classical computer will ever get — is the set of **endpoints** of those paths: the fired detectors, each one a little flag planted at a spacetime coordinate (which check, which round). Chains still end in pairs, and they can still end singly at a boundary — now including the *time* boundaries: the first round and the last round of the experiment act like extra edges of the board, able to absorb a lone endpoint.

A sketch of the stack, with time running upward. One data-qubit X error lands between rounds 2 and 3 (a space-like edge, firing detectors A and B on two checks); one measurement lies during round 4 (a time-like edge, firing C when the lie starts and D when it ends):

```
round 5        .    .    .    .
                        |
round 4        .    .    D    .      <- lie ends: detector fires again
                        |
              (measurement error: a time-like chain)
                        |
round 3        .    .    C    .      <- lie begins: detector fires
round 2        .    A----B    .      <- error lands: detectors fire
              (data error: a space-like chain)
round 1        .    .    .    .
```

Four flags, two hidden chains. The decoder receives only the four flags and must sketch the chains for itself — that reconstruction is the next lesson's game.

## The maze analogy

Here is a picture worth carrying around. The spacetime stack is a multi-storey **hedge maze**, one storey per round. Somewhere inside, invisible walkers are on the move — the error chains. You are a detective posted outside, and you never see the walkers or the corridors. All you have is a log of **door events**: each walker, passing through a corridor, pushes a door open at each end. Your job is to reconstruct the likeliest routes from the door log alone.

Now add the realistic twist: the door sensors are flaky, and sometimes report a phantom opening. Naively you would chase phantoms forever. But you notice that a real passage leaves a door *changed* — it was closed, now it is open — while a phantom report is a blip that contradicts the very next reading. So you stop trusting the doors' reported state and start trusting *transitions* in the state. A phantom shows up in your transition log as open-then-immediately-closed: a pair of events in time, at the same door — which your reconstruction treats as just another short route to explain, one climbing straight up a storey.

Every piece of the analogy is a real term: the maze is the spacetime volume, the walkers are error chains, the door-transition events are detection events, and "reconstruct the likeliest routes" is decoding — the subject of the next two lessons.

## Why not just build better referees?

A fair question: if measurement noise is the problem, why not engineer it away instead of adding a whole dimension? Two answers, one practical and one fundamental.

The practical answer is that measurements are already among the slowest, most failure-prone operations on every quantum platform we have, and making them dramatically more reliable is much harder than repeating them. Repetition converts an impossible hardware demand (trustworthy single-shot referees) into a software problem (compare streams of readings) — and software problems are the kind this field knows how to solve.

The fundamental answer is that the trick does not merely tolerate noisy referees; it *absorbs them into the same framework* as everything else. A measurement error is not a special nuisance requiring special handling — it is an error chain pointing in the time direction, decoded by the same machinery, with the same pair rule, as a data error. One theory covers every fault. That unity is why the spacetime picture, and not the flat chessboard, is the working representation inside every real decoder and every compiler you will meet in tiers 3 and 5.

## How many rounds?

Repetition raises an obvious worry: if a single measurement lies 1% of the time, and you need the check's value *now*, how many rounds of comparing does it take to be sure? The answer is built into the architecture: about **d rounds** of measurements are grouped together per decoding unit for a distance-d patch — the same distance that protects space protects time. Intuitively, a measurement lie is a chain one round long; to fool the code, lies would have to chain across all d rounds the way data errors would have to chain across d rows. Time gets the same exponential protection as space, for the same price: d² qubits in space, d rounds in time — a d×d×d cube of spacetime as the atom of fault-tolerant memory. (The tier 3 lessons will literally draw computation as rearrangements of such cubes.)

This is also why *cycle time* matters so much to hardware builders. The round — extract every check once — is the clock tick of the whole machine: superconducting chips tick in about a microsecond, ion traps in about a millisecond. Every decoding budget, every factory schedule, every latency number quoted later in this lab is denominated in these ticks.

## Reading a detector pattern, slowly

Practice the spacetime interpretation once, the way the anyon lesson practiced the flat one. Suppose your detector log for one patch shows exactly four events: check 7 fired between rounds 10 and 11, check 12 fired between rounds 10 and 11, and check 7 also fired between rounds 20 and 21 and again between 21 and 22.

- The first pair: two checks, same round boundary, a few squares apart — the classic signature of a short **data-error chain** on the board between rounds 10 and 11. Space-like.
- The second pair: the *same* check, firing at two *consecutive* round boundaries — the check's reading flipped and flipped back. That is a **measurement lie** during round 21. Time-like.
- Could the round-21 pair instead be two separate data errors at check 7? Possible but improbable: it needs two independent errors on exactly the right qubits one round apart, while the lie needs one. Decoders systematically prefer the cheap explanation — and quantifying "cheap" is the entire next lesson.

You have just read a spacetime syndrome: four flags, two chains, one in space and one in time. A real decoder does this for thousands of flags per round, a million rounds per second.

## What the decoder actually receives

Strip the analogy away and the decoding problem is this. **Input:** a scatter of detection events in a 3D spacetime graph, accumulated round by round. **Hidden truth:** some set of error chains whose endpoints are exactly that scatter. **Job:** guess a set of chains — a *correction* — such that the correction combined with the true error forms only closed loops.

Why closed loops? Because a loop of errors that starts and ends at no boundary is a product of stabilizers: it acts on the logical qubit as the identity, so correcting along it is harmless. The decoding succeeds precisely when correction-plus-error is **homologically trivial** — all loops, nothing spanning. It fails when the guess pairs the events so badly that correction-plus-error leaves a chain running from boundary to boundary: an invisible logical operator, applied by accident. You built one of those by hand in the distance lesson; the decoder's whole purpose is to never build one by mistake.

This framing is not folklore. Dennis, Kitaev, Landahl, and Preskill made it exact in 2001 ([quant-ph/0110143](https://arxiv.org/abs/quant-ph/0110143)): choosing the best correction is a statistical-mechanics problem on the spacetime graph, and whether the code works at all is a question about that model's *phase transition*. That result is the foundation the next lesson stands on.

## One honest caveat

Detectors compare consecutive rounds, so a detector cannot fire in the very first round (there is nothing to compare against) — the time boundaries handle that, as described above. Also, detectors as defined here assume the same check measured the same way each round; the fancier fault-tolerant circuits of later tiers keep the same principle — compare, trust changes — with more bookkeeping.

## Terminology check

The literature uses three words for overlapping ideas, and papers assume you have them sorted:

- The **syndrome** is, strictly, the collection of check measurement outcomes — what the referees reported. Loosely (and commonly) people use it for whatever the decoder sees.
- A **symptom** or **detection event** is one change in that stream — one fired detector. The symptoms, not the raw syndrome, are the endpoints the decoder matches.
- A **detector** is the comparison itself: the little parity machine (this check, these two rounds) that can fire. A patch has a fixed set of detectors; each round of operation offers each of them a chance to produce a symptom.

When a paper says "the syndrome is sparse below threshold," it means symptoms are rare. When a tool input file lists "detectors," it means the parity machines. Same movie, three camera angles.

## Key numbers

- 1 new dimension: the decoding arena is a 3D spacetime graph — 2 space + 1 time — not the flat board.
- A detector compares 2 consecutive measurements of 1 check; a distance-d rotated patch contributes d²−1 detectors per round-pair (8 for d = 3, 24 for d = 5).
- A data-qubit error fires detectors in a spatial pair; a measurement error fires a pair in time (lie begins, lie ends). Singles only at a boundary — spatial or temporal.
- Decoding = guessing chains from endpoints; success = correction plus error forms closed loops only; failure = a residual chain spanning boundary to boundary.

## Next

The decoder now has its input: a scatter of endpoints in spacetime, and the knowledge that short chains are more likely than long ones. Time to build the machine that pairs them up — [minimum-weight perfect matching](#/lesson/mwpm-decoding).
