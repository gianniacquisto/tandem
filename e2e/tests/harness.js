// @ts-check
const { spawn, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const readline = require('node:readline');

function resolveBinary() {
  if (process.env.TANDEM_BIN) {
    return path.resolve(process.env.TANDEM_BIN);
  }
  const exe = process.platform === 'win32' ? 'tandem.exe' : 'tandem';
  const debugBin = path.resolve(__dirname, '..', '..', 'rust', 'target', 'debug', exe);
  if (fs.existsSync(debugBin)) {
    return debugBin;
  }
  const releaseBin = path.resolve(__dirname, '..', '..', 'rust', 'target', 'release', exe);
  if (fs.existsSync(releaseBin)) {
    return releaseBin;
  }
  throw new Error('Could not find the built tandem binary under rust/target/debug or rust/target/release. Run `cargo build -p tandem` in rust/ first.');
}

function runCli(bin, home, userHome, args) {
  const env = { ...process.env, TANDEM_HOME: home, HOME: userHome };
  delete env.TANDEM_AGENT;
  delete env.TANDEM_TOKEN;
  const out = execFileSync(bin, ['--home', home, ...args], {
    env,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return out;
}

/**
 * Starts a sandboxed `tandem serve --port 0 --host 127.0.0.1` instance,
 * seeded with one project ("Tandem"), one label ("ui"), one Backlog ticket,
 * and one note ("Index.md").
 */
async function startServer() {
  const bin = resolveBinary();
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'tandem-e2e-'));
  const home = path.join(root, 'home');
  const userHome = path.join(root, 'user');
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(userHome, { recursive: true });

  const cli = (args) => runCli(bin, home, userHome, args);

  cli(['project', 'add', '--name', 'Tandem']);
  cli(['label', 'add', '--project', 'Tandem', '--name', 'ui']);
  const ticketRaw = cli(['ticket', 'add', '--project', 'Tandem', '--label', 'ui', '--title', 'First card', '--description', 'Initial brief']);
  const ticket = JSON.parse(ticketRaw);
  const notePath = path.join(home, 'notes', 'Index.md');
  cli(['note', 'create', '--path', 'Index.md']);
  fs.writeFileSync(notePath, '# Welcome\nInitial note line\n');
  const readNote = () => fs.readFileSync(notePath, 'utf8');
  const writeNote = (text) => fs.writeFileSync(notePath, text);

  const env = { ...process.env, TANDEM_HOME: home, HOME: userHome };
  delete env.TANDEM_AGENT;
  delete env.TANDEM_TOKEN;

  const child = spawn(bin, ['--home', home, 'serve', '--host', '127.0.0.1', '--port', '0'], {
    env,
    stdio: ['ignore', 'pipe', 'inherit'],
  });

  const rl = readline.createInterface({ input: child.stdout });
  const baseURL = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error('tandem serve did not print its listening line within 10s'));
    }, 10000);

    rl.on('line', (line) => {
      const match = /(http:\/\/127\.0\.0\.1:\d+)/.exec(line);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });

    child.once('error', (err) => {
      clearTimeout(timer);
      reject(err);
    });

    child.once('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`tandem serve exited early with code ${code}`));
    });
  });

  // Keep draining stdout so the pipe buffer never stalls the server.
  rl.on('line', () => {});

  async function stop() {
    rl.close();
    if (child.exitCode === null) {
      child.kill('SIGTERM');
      await new Promise((resolve) => {
        const killTimer = setTimeout(() => {
          if (child.exitCode === null) child.kill('SIGKILL');
          resolve();
        }, 2000);
        child.once('exit', () => {
          clearTimeout(killTimer);
          resolve();
        });
      });
    }
    fs.rmSync(root, { recursive: true, force: true });
  }

  return {
    baseURL,
    home,
    ticket,
    cli,
    readNote,
    writeNote,
    stop,
  };
}

module.exports = { startServer };
