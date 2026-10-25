const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

const noteId = '00000000-0000-4000-8000-000000000002';
const repoId = '00000000-0000-4000-8000-000000000001';
function file(name, id = noteId, repository = repoId) {
  return { path: name, extension: 'md', stat: {size: 128}, text: `---\nskillshub_note_id: ${id}\nskillshub_repository_id: ${repository}\n---\nBody` };
}
function fixture(files) {
  const notices = [], opened = [], copied = [], external = [], events = {}, handlers = {}, commands = {};
  let layoutReady;
  class Plugin {
    registerEvent() {}
    registerObsidianProtocolHandler(action, handler) { handlers[action] = handler; }
    addCommand(command) { commands[command.id] = command.callback; }
  }
  const module = {exports: {}};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../main.js'), 'utf8'), {
    module, exports: module.exports,
    require(name) {
      assert.equal(name, 'obsidian');
      return { Plugin, TFile: class {}, Notice: class {constructor(message) { notices.push(message); }},
        getFrontMatterInfo(text) { const match = /^---\n([\s\S]*?)\n---/.exec(text); return {exists: !!match, frontmatter: match?.[1]}; },
        parseYaml(text) { return Object.fromEntries(text.split('\n').map(line => line.split(': '))); },
      };
    },
    navigator: {clipboard: {writeText: async value => {copied.push(value);}}},
    window: {open: url => external.push(url)},
  });
  const plugin = new module.exports.default();
  plugin.app = {
    workspace: {onLayoutReady(callback) {layoutReady = callback;}, getLeaf: () => ({openFile: async f => opened.push(f.path)}), getActiveFile: () => files[0]},
    vault: {on(name, callback) {events[name] = callback;}, getMarkdownFiles: () => files, cachedRead: async f => f.text,
      getAbstractFileByPath: name => files.find(f => f.path === name)},
  };
  plugin.onload();
  return {plugin, notices, opened, copied, external, events, handlers, commands, ready: () => layoutReady()};
}
const settle = () => new Promise(resolve => setImmediate(resolve));

test('cold-start URI waits for layout, then opens by UUID without any path parameter', async () => {
  const f = fixture([file('Different folder/任意名称.md')]);
  f.handlers['skillshub-note']({id: noteId}); await settle();
  assert.deepEqual(f.opened, []);
  f.ready(); await settle();
  assert.deepEqual(f.opened, ['Different folder/任意名称.md']);
});

test('the same URI works after rename and move; duplicate and deleted notes are rejected', async () => {
  const files = [file('original.md')]; const f = fixture(files); f.ready();
  f.handlers['skillshub-note']({id: noteId}); await settle();
  files[0].path = 'Moved/renamed.md'; f.events.rename();
  f.handlers['skillshub-note']({id: noteId}); await settle();
  assert.deepEqual(f.opened, ['original.md', 'Moved/renamed.md']);
  files.push(file('duplicate.md')); f.events.create();
  f.handlers['skillshub-note']({id: noteId}); await settle();
  assert.match(f.notices.at(-1), /Duplicate/); assert.equal(f.opened.length, 2);
  files.splice(0); f.events.delete();
  f.handlers['skillshub-note']({id: noteId}); await settle();
  assert.match(f.notices.at(-1), /not found/); assert.equal(f.opened.length, 2);
});

test('commands copy path-free links and construct only safe Arsenal navigation URIs', async () => {
  const files = [file('note.md')]; const f = fixture(files); f.ready();
  f.commands['copy-stable-note-link'](); await settle();
  assert.deepEqual(f.copied, [`obsidian://skillshub-note?id=${noteId}`]);
  f.commands['return-to-arsenal'](); await settle();
  assert.deepEqual(f.external, [`skillshub://repository/${repoId}?tab=notebook`]);
  files[0].text = '---\nskillshub_repository_id: javascript:alert(1)\n---\n';
  f.commands['return-to-arsenal'](); await settle(); assert.equal(f.external.length, 1);
  f.handlers['skillshub-note']({id: '../unsafe'}); await settle(); assert.equal(f.opened.length, 0);
});

test('modifying an identity invalidates the index instead of opening the previous match', async () => {
  const files = [file('note.md')]; const f = fixture(files); f.ready();
  f.handlers['skillshub-note']({id: noteId}); await settle();
  files[0].text = files[0].text.replace(noteId, repoId); f.events.modify();
  f.handlers['skillshub-note']({id: noteId}); await settle();
  assert.equal(f.opened.length, 1); assert.match(f.notices.at(-1), /not found/);
});
