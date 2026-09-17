import { EventKind, TaskEvent, TaskRecord } from '../types.js';

export interface EventLimits {
  maxEventsPerTask: number;
  maxEventChars: number;
  maxEventsPerPage: number;
}

function clip(text: string, max: number): string {
  const flat = text.replace(/\s+/g, ' ').trim();
  return flat.length > max ? `${flat.slice(0, max)}... [${flat.length - max} more characters]` : flat;
}

/**
 * Appends one event and keeps the buffer bounded.
 *
 * Dropped events are counted rather than forgotten, so a caller whose cursor has fallen
 * off the back of the window is told that it happened instead of silently skipping work.
 */
export function appendEvent(task: TaskRecord, kind: EventKind, text: string, limits: EventLimits): void {
  const trimmed = clip(text, limits.maxEventChars);
  if (!trimmed) return;
  task.events.push({ seq: task.nextSeq++, at: new Date().toISOString(), kind, text: trimmed });
  const overflow = task.events.length - limits.maxEventsPerTask;
  if (overflow > 0) {
    task.events.splice(0, overflow);
    task.droppedEvents += overflow;
  }
  task.updatedAt = new Date().toISOString();
}

export interface EventPage {
  events: TaskEvent[];
  next_cursor: number;
  has_more: boolean;
  /** Set when the requested cursor was older than anything still retained. */
  dropped_before_cursor?: number;
}

/** Returns events after `cursor`, at most one page, plus the cursor to ask with next. */
export function pageEvents(task: TaskRecord, cursor: number, limits: EventLimits): EventPage {
  const oldest = task.events[0]?.seq ?? task.nextSeq;
  const after = task.events.filter(event => event.seq >= cursor);
  const page = after.slice(0, limits.maxEventsPerPage);
  const result: EventPage = {
    events: page,
    next_cursor: page.length ? page[page.length - 1]!.seq : Math.max(cursor, task.nextSeq - 1),
    has_more: after.length > page.length
  };
  if (cursor > 0 && cursor + 1 < oldest && task.droppedEvents > 0) {
    result.dropped_before_cursor = oldest - cursor - 1;
  }
  return result;
}
