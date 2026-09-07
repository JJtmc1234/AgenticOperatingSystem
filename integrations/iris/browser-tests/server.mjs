// Local test transport for the real portal handlers and SQLite statements.
import http from 'node:http';
import { DatabaseSync } from 'node:sqlite';
import { readFileSync } from 'node:fs';
import worker from './portal/worker.js';
import { hashOf } from './portal/people.js';
const database = new DatabaseSync(':memory:');
database.exec(readFileSync(new URL('./portal/schema.sql', import.meta.url), 'utf8'));
const DB = {
  prepare(sql) {
    let args = [];
    const statement = database.prepare(sql);
    const query = {
      bind(...values) { args = values; return query; },
      async first() { return statement.get(...args) ?? null; },
      async all() { return { results: statement.all(...args) }; },
      async run() { statement.run(...args); return { success: true }; },
    };
    return query;
  },
};
const env = { DB, OWNER: 'JJ', PASSWORDS: JSON.stringify({
  JJ: await hashOf('fixture-owner-only'), Hunter: await hashOf('fixture-member-only'),
}) };
http.createServer(async (req, res) => {
  try {
    const chunks = [];
    for await (const chunk of req) chunks.push(chunk);
    const body = Buffer.concat(chunks);
    if (req.url === '/__reset' && req.method === 'POST') {
      database.exec('DELETE FROM said; DELETE FROM asked; DELETE FROM person;');
      res.writeHead(204).end(); return;
    }
    const request = new Request('http://127.0.0.1:18787' + req.url, {
      method: req.method, headers: req.headers,
      ...(body.length ? { body } : {}),
    });
    const response = await worker.fetch(request, env);
    res.writeHead(response.status, Object.fromEntries(response.headers));
    res.end(Buffer.from(await response.arrayBuffer()));
  } catch (error) {
    console.error(error);
    res.writeHead(500).end('Local test server error');
  }
}).listen(18787, '127.0.0.1');
