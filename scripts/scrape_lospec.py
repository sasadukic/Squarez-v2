#!/usr/bin/env python3
"""Scrape popular Lospec palettes and save to JSON for embedding in Squarez."""

import json
import urllib.request
import urllib.error
import time
from pathlib import Path

# Curated list of well-known popular pixel art palette slugs on Lospec.
# These are some of the most downloaded/used palettes.
POPULAR_SLUGS = [
    # Classic / widely used
    "pico-8", "aap-64", "lospec500", "edg16", "edg32", "edg64",
    "endesga-16", "endesga-32", "endesga-36", "endesga-64", "endesga-128",
    "apollo", "blk-nex", "bubblegum-16", "copper-tech", "dawnbringer-16",
    "dawnbringer-32", "fairy-tone", "faraway", "flevy-pal", "gameboy-color",
    "green-pest", "hard-sect", "hugo-7", "kirokaze", "maroon",
    "microsoft-windows", "mist-gb", "moonlight-b16", "na16", "nostalgia",
    "nyx8", "oil-6", "onebit-monitor-glow", "parchment", "pearl-36",
    "phoenix-4", "pixel-couple-8", "pokemon-sgb", "princess-peach",
    "r-d-g-b", "simplejpc-16", "slso8", "soapy-10", "steam-lords",
    "sweetie-16", "vinik-24", "wish-gb", "winter-gb", "warm-16",
    "warp-tart", "wednesday", "wood-8", "yk-24", "zughy-32",
    # Additional popular ones
    "desatur8", "sup3rl1minal", "ed-vision", "sundown-12", "xarss16",
    "memory-block-36", "polandal", "circadian24", "stanky-ghost",
    "gunsgax24", "ferris-wheel-6", "circus", "dancing-sprite",
    "dream-gb", "elevate", "equinox-8", "grayscale-1bit",
    "ink", "judge", "kr-tierno", "lospec-ls-1", "mellow-fruit",
    "milkshake-16", "mini-4", "neapolitan", "new-world", "nostalgic-8",
    "nyx16", "oil-12", "paper-8", "pastel-13", "pepto",
    "pixelbit", "platinum", "poison", "psilk", "rad-squad",
    "retro-8", "retro-calculator", "rustic-garden", "shimapan",
    "simple-mushroom", "snowy-forest", "space-blue", "splat",
    "st-8", "sunset", "sushi", "swap", "swordtario",
    "tea", "toxic", "undead", "universal-8", "vivid",
    "wari", "wheat", "wilderness", "winter-8", "wisteria",
    "yoghurt", "z", "zephyr", "zombie",
    # More classics
    "arne-16", "arne-32", "cc-29", "cg-arne", "color-pencil",
    "curios-mage", "double-diver", "earthbound", "fall-15", "feather-36",
    "flesh-in-vaar", "florentine", "four-bit-four", "gbpocket", "ghost-of-the-arcade",
    "grayscale-16", "hept-32", "herm-liege", "hollow", "ice-cream-gb",
    "island-wanderer", "ketchup-mustard", "kirokaze-32", "lava-gb",
    "lost-century", "lux2", "m-airlines", "macaw", "mangavania",
    "marshmallow-32", "mellow-yellow", "midnight-plum", "milky-32",
    "mistigrey", "mizu", "monster", "msx", "na-128-colors",
    "nintendo-gameboy-bgb", "nymph-7", "pastel-11", "pastel-gb",
    "patched-22", "pear36", "pixel-ink", "pocket-girl", "polar-11",
    "purples-2", "raspberry", "resurrect-64", "rosy-42", "rustic-gb",
    "sage-12", "secret-cube", "secretninjart", "shiny-ao", "sodie-pop",
    "spacehaze", "speccy", "spooky-8", "stdgb",
    "striker", "summer-solstice", "super-famicom-style", "super-paper-mario",
    "supernova-7", "sweet-16", "taffy-16", "tamgapack", "tango",
    "thraxdemoon", "toad", "tofu-20", "vapor-12", "vivid-chalk",
    "voodoocastle", "wan-24", "watercoat", "winter", "witchy",
    "wood", "xaiy-blue", "xaiy-green", "xaiy-grey", "xaiy-red",
    "zanzlanz-16", "zughy-16",
]

def fetch_palette(slug):
    """Fetch a single palette from Lospec's public JSON endpoint."""
    url = f"https://lospec.com/palette-list/{slug}.json"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "Squarez-Scraper/1.0"})
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            return {
                "slug": slug,
                "name": data.get("name", slug),
                "author": data.get("author", ""),
                "colors": data.get("colors", []),
            }
    except urllib.error.HTTPError as e:
        if e.code == 404:
            print(f"  [404] {slug}")
        else:
            print(f"  [HTTP {e.code}] {slug}")
        return None
    except Exception as e:
        print(f"  [ERR] {slug}: {e}")
        return None

def main():
    out_path = Path(__file__).parent.parent / "assets" / "lospec_palettes.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)

    palettes = []
    for i, slug in enumerate(POPULAR_SLUGS):
        print(f"[{i+1}/{len(POPULAR_SLUGS)}] {slug}...", end=" ", flush=True)
        pal = fetch_palette(slug)
        if pal:
            palettes.append(pal)
            print(f"OK ({len(pal['colors'])} colors)")
        else:
            print("FAIL")
        time.sleep(0.15)  # be nice to lospec

    # Sort by name for consistency
    palettes.sort(key=lambda p: p["name"].lower())

    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(palettes, f, indent=2)

    print(f"\nSaved {len(palettes)} palettes to {out_path}")

if __name__ == "__main__":
    main()
