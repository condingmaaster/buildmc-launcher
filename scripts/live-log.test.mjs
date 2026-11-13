import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';

const source = readFileSync(new URL('../src/lib/liveLog.ts', import.meta.url), 'utf8');
const { outputText } = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } });
const { watchLog } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`);
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
async function until(condition) {
  const deadline = Date.now() + 2000;
  while (!condition()) {
    assert.ok(Date.now() < deadline, 'Log-Aktualisierung blieb aus');
    await tick();
  }
}
function start(t, read, publish) {
  const live = watchLog(read, publish, 5);
  t.after(() => live.dispose());
  return live;
}

test('aktualisiert ohne Live-Ereignisse und erkennt Ringpuffer gleicher Länge', async t => {
  let buffer = ['eins'];
  let shown;
  let updates = 0;
  start(t, async () => [...buffer], lines => { shown = lines; updates++; });
  await until(() => shown?.[0] === 'eins');
  buffer = ['zwei'];
  await until(() => shown?.[0] === 'zwei');
  await tick();
  await tick();
  assert.equal(updates, 2, 'identische Puffer dürfen kein erneutes Rendern auslösen');
});

test('übernimmt identische aufeinanderfolgende Logzeilen vollständig', async t => {
  let buffer = ['wiederholt'];
  let shown;
  start(t, async () => [...buffer], lines => { shown = lines; });
  await until(() => shown?.length === 1);
  buffer.push('wiederholt');
  await until(() => shown?.length === 2);
  assert.deepEqual(shown, buffer);
});

test('holt nach einem IPC-Fehler neue Zeilen nach', async t => {
  let requests = 0;
  let shown;
  start(t, async () => {
    if (++requests === 1) throw Error('kurz unterbrochen');
    return ['wieder verbunden'];
  }, lines => { shown = lines; });
  await until(() => shown?.[0] === 'wieder verbunden');
});

test('ignoriert verspätete Antworten nach Instanzwechsel oder Schließen', async t => {
  let finish;
  let updates = 0;
  const live = start(t, () => new Promise(resolve => { finish = resolve; }), () => updates++);
  live.dispose();
  finish(['alte Instanz']);
  await tick();
  assert.equal(updates, 0);
});

test('Leeren wird nicht durch einen älteren Abruf rückgängig gemacht', async t => {
  let finish;
  let first = true;
  let buffer = ['alt'];
  let shown;
  const live = start(t, () => {
    if (first) {
      first = false;
      return new Promise(resolve => { finish = resolve; });
    }
    return Promise.resolve([...buffer]);
  }, lines => { shown = lines; });
  await live.clear(async () => { buffer = []; });
  finish(['alt']);
  await tick();
  assert.deepEqual(shown, []);
  buffer = ['nach Leeren'];
  await until(() => shown?.[0] === 'nach Leeren');
});

test('startet bei langsamer Antwort keine überlappenden Abrufe', async t => {
  let finish;
  let requests = 0;
  const live = start(t, () => {
    requests++;
    return new Promise(resolve => { finish = resolve; });
  }, () => {});
  await tick();
  await tick();
  assert.equal(requests, 1);
  live.dispose();
  finish([]);
});
