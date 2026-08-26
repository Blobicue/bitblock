# The one-bit gallery

The README parks the hierarchical renderer "after this interaction model is
judged." Judgement needs artifacts. This gallery is four of them — four
theses about what a binary coordinate space can express, each a `.bb`
program rendered windowlessly through the same public path the app uses
(`Program::parse` → `World` → `Camera` → `Renderer`), each reproducible:

```powershell
python docs/gallery/generate.py
cargo run --release --example gallery_frame -- docs/gallery/<name>.bb docs/gallery/<name>.png [yaw-deg] [pitch-deg]
```

The renderer prints the exact camera and on-bit count for every frame. The
orange cube in every image is the origin bedrock — always on, unbreakable.

## The theses

**symbol.png — bits carry SYMBOLS.** The word BIT in a 5x7 font, extruded
two bits deep, standing beside the bedrock. 336 instructions. It reads at
a glance: letters, serifs, both bowls of the B. A one-bit world can say
things.

**recursion.png — bits carry RECURSION.** A level-2 Menger sponge (400 of
729 cells, 55% air), centered on the origin so the bedrock sits inside the
sponge's hollow heart — visible, orange, through the aligned central
shaft. 25,600 instructions. Rendered dead-frontal; see the finding below
for why. The permanent reference point enshrined at the center of the
recursion is the project's founding myth in one frame.

**recognition.bb — bits carry RECOGNITION.** The classic 16x16 smiley,
extruded. 552 instructions. The strongest human hook: you cannot look at
it and not see a face. No color, no material, no shading on the bits
themselves — the recognition is carried by the on/off pattern alone.

**architecture.bb — bits carry ARCHITECTURE.** A gate: two piers, a
lintel, a pediment, a threshold, steps descending toward the observer —
and a walkthrough opening you can put the camera through. 662
instructions. This is the Minecraft instinct reduced to its skeleton: a
place, not just a pattern. Walk the observer through it before judging
the interaction model.

## What building the gallery taught (measured, not vibes)

Two failures taught more than the successes, and both are laws of the
medium:

1. **Dense recursion reads as solid.** The first attempt was a level-3
   sponge (8,000 cells). Its face holes open onto interior white walls,
   and from any oblique angle the whole object renders as a solid mass
   with corner notches. Level 2 was chosen because its recursion is
   *visible*.
2. **Interior depth is alignment-dependent.** A through-shaft shows
   background only when the camera is near its axis; a few degrees of
   skew over 36 bits of depth seals a 4-bit shaft completely (measured:
   a wall probe renders holes exactly; the sponge's sub-shafts sealed
   until the camera was placed on the central axis). In this medium,
   **distance reveals and proximity seals** — the opposite of the
   instinct to fly closer to look inside. Any future interaction design
   should know this: one-bit worlds hide their interiors from every
   angle but the aligned ones.

## The open verdict

The theses are argued by the artifacts, not settled. What an agent can
verify is that the frames render, reproduce, and read at terminal
resolution. What only a human can judge is whether any of this is worth
*building in* — whether the hand wants to place the next bit. That
judgement is the deferred checkpoint, and this gallery is its evidence
packet. Open the app, load `architecture.bb`, walk through the gate, and
decide.
