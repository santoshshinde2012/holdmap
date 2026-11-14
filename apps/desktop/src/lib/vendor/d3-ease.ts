// d3-transition imports only `easeCubicInOut` from d3-ease. d3-ease 3.0.2 ships a single
// bundled module whose elastic, back and poly easings are built by immediately invoked
// functions, which a bundler has to keep, so ~2 KB of unused easings rode along with every
// graph. vite.config.ts points `d3-ease` here instead.
//
// From d3-ease src/cubic.js. Copyright 2010-2021 Mike Bostock. ISC License:
// Permission to use, copy, modify, and/or distribute this software for any purpose with or
// without fee is hereby granted, provided that the above copyright notice and this permission
// notice appear in all copies. THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL
// WARRANTIES WITH REGARD TO THIS SOFTWARE.

/** Cubic ease in and out, d3's default transition easing. */
export function easeCubicInOut(t: number): number {
  return ((t *= 2) <= 1 ? t * t * t : (t -= 2) * t * t + 2) / 2;
}
