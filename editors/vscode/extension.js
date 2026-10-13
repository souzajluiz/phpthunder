const fs = require('fs');
const path = require('path');
const vscode = require('vscode');
const { LanguageClient } = require('vscode-languageclient/node');

let client;

function serverPath(context) {
  const configured = vscode.workspace.getConfiguration('phpthunder').get('path');
  if (configured) return configured;
  const local = path.join(context.extensionPath, '..', '..', 'target', 'release', 'phpthunder');
  return fs.existsSync(local) ? local : 'phpthunder';
}

function activate(context) {
  const command = serverPath(context);
  client = new LanguageClient(
    'phpthunder',
    'phpthunder',
    { command, args: [] },
    {
      documentSelector: [{ scheme: 'file', language: 'php' }],
    },
  );
  client.start();
  context.subscriptions.push({ dispose: () => client && client.stop() });
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
