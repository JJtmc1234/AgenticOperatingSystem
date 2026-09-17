import type { EntityProto } from '../factorio/types';
import type { BlueprintEntity, Bounds } from '../blueprint/model';

export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** Footprint used when no prototype data is available for an entity. */
export const UNKNOWN_FOOTPRINT: Rect = { x: -0.5, y: -0.5, width: 1, height: 1 };

/**
 * World-space box an entity occupies.
 *
 * Factorio rotates the selection box with the entity, so an east or west
 * facing entity has its width and height swapped. The diagonal directions
 * only occur on rails, whose boxes are square, so they need no special case.
 */
export function entityRect(entity: BlueprintEntity, proto: EntityProto | undefined): Rect {
  if (!proto) {
    return { x: entity.x - 0.5, y: entity.y - 0.5, width: 1, height: 1 };
  }
  const { width, height, offsetX, offsetY } = proto.footprint;
  const sideways = entity.direction === 4 || entity.direction === 12;
  const w = sideways ? height : width;
  const h = sideways ? width : height;
  const cx = entity.x + (sideways ? offsetY : offsetX);
  const cy = entity.y + (sideways ? offsetX : offsetY);
  return { x: cx - w / 2, y: cy - h / 2, width: w, height: h };
}

export function rectsOverlap(a: Rect, b: Rect): boolean {
  return (
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
  );
}

export function rectContains(r: Rect, x: number, y: number): boolean {
  return x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
}

/** Grow a rect outwards by `d` tiles on every side. */
export function inflate(r: Rect, d: number): Rect {
  return { x: r.x - d, y: r.y - d, width: r.width + d * 2, height: r.height + d * 2 };
}

export function unionBounds(rects: Iterable<Rect>): Bounds {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  let any = false;
  for (const r of rects) {
    any = true;
    if (r.x < minX) minX = r.x;
    if (r.y < minY) minY = r.y;
    if (r.x + r.width > maxX) maxX = r.x + r.width;
    if (r.y + r.height > maxY) maxY = r.y + r.height;
  }
  return any ? { minX, minY, maxX, maxY } : { minX: 0, minY: 0, maxX: 1, maxY: 1 };
}

/** The tile a world point falls in. */
export function tileOf(x: number, y: number): [number, number] {
  return [Math.trunc(x), Math.trunc(y)];
}
