import { describe, expect, it } from "vitest";
import { cardInside, cardZoom, centeredView, clampZoom, fitZoom, pinnedView, tooWide } from "./flow-camera";

const box = { x: 100, y: 40, width: 800, height: 600 };

describe("flow camera", () => {
  it("fits a picture into the pane left after the chrome", () => {
    expect(fitZoom(box, 400, 300)).toBe(0.5);
    expect(fitZoom({ width: 0, height: 600 }, 400, 300)).toBe(0.5);
  });

  it("clamps a fit between a floor and a ceiling", () => {
    expect(clampZoom(fitZoom(box, 950, 780), 0.08, 1.1)).toBe(1.1);
    expect(clampZoom(fitZoom(box, 200, 150), 0.08, 1.1)).toBe(0.25);
    expect(clampZoom(0.02, 0.08, 1.1)).toBe(0.08);
  });

  it("centers the service graph inside the toolbar and control insets", () => {
    const view = centeredView(box, 0.85, { left: 24, top: 84, width: 950, height: 640 });
    expect(view.zoom).toBe(0.85);
    expect(view.x).toBe(24 + (950 - 800 * 0.85) / 2 - 100 * 0.85);
    expect(view.y).toBe(84 + (640 - 600 * 0.85) / 2 - 40 * 0.85);
  });

  it("pins the calls graph on the first participants", () => {
    expect(pinnedView(box, 0.8, { x: 28, y: 84 })).toEqual({ x: 28 - 80, y: 84 - 32, zoom: 0.8 });
  });

  it("treats a picture as too wide when the readable zoom no longer fits", () => {
    expect(tooWide(box, { width: 1000, height: 700 }, 0.8, 56, 112)).toBe(false);
    expect(tooWide(box, { width: 600, height: 700 }, 0.8, 56, 112)).toBe(true);
    expect(tooWide({ width: 400, height: 900 }, { width: 1000, height: 700 }, 0.8, 56, 112)).toBe(true);
  });

  it("leaves a card alone when it is already readable and on screen", () => {
    const card = { width: 236, height: 72 };
    const pane = { width: 1000, height: 800 };
    const inset = { left: 20, right: 20, top: 20, bottom: 64 };
    const onScreen = { x: 400, y: 300, zoom: 0.85 };
    expect(cardInside(onScreen, { x: 200, y: 200 }, card, pane, inset, 0.85)).toBe(true);
    expect(cardInside({ ...onScreen, zoom: 0.7 }, { x: 200, y: 200 }, card, pane, inset, 0.85)).toBe(false);
    expect(cardInside(onScreen, { x: -400, y: 200 }, card, pane, inset, 0.85)).toBe(false);
  });

  it("jumps to a card without shrinking the type below readable", () => {
    expect(cardZoom(1, 0.85, 2)).toBe(1);
    expect(cardZoom(0.4, 0.85, 2)).toBe(0.85);
    expect(cardZoom(1.4, 0.85, 1.1)).toBe(1.1);
    expect(cardZoom(1.4, 0.85, 0.5)).toBe(0.85);
  });
});
