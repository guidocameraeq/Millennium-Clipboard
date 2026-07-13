// Hook PostToolUse (Edit|Write): valida sintaxis del archivo recién editado según su extensión.
// Si el chequeo falla, exit 2 → el error le llega a Claude al instante, antes de que el
// código roto viaje a ningún lado.
//
// Va como .cjs a propósito: el package.json del proyecto tiene "type": "module", así que un
// .js se cargaría como ES module y este require() explotaría. La extensión .cjs fuerza CommonJS.
//
// STACK ACTIVO: .js (node --check) y .sh (bash -n). Rust NO se chequea acá: `cargo check`
// compila el crate entero (lento para correr en cada edición) — eso vive en /smoke y en la
// verificación de cada fase. Si una herramienta no está instalada, el hook se salta el
// chequeo en silencio (no rompe nada por defecto).

const path = require('path');
const { execFileSync } = require('child_process');

// Mapa extensión → [comando, args]. '{file}' se reemplaza por el path editado.
const CHECKS = {
  '.sh': ['bash', ['-n', '{file}']],
  '.js': ['node', ['--check', '{file}']],
};

let raw = '';
process.stdin.on('data', (c) => (raw += c));
process.stdin.on('end', () => {
  let file = '';
  try {
    file = String(JSON.parse(raw).tool_input?.file_path || '');
  } catch {
    process.exit(0);
  }
  const ext = path.extname(file).toLowerCase();
  const check = CHECKS[ext];
  if (!check) process.exit(0);

  const [cmd, args] = check;
  const finalArgs = args.map((a) => a.replace('{file}', file));
  try {
    execFileSync(cmd, finalArgs, {
      stdio: ['ignore', 'ignore', 'pipe'],
      cwd: process.env.CLAUDE_PROJECT_DIR || undefined,
    });
    process.exit(0);
  } catch (e) {
    if (e.code === 'ENOENT') process.exit(0); // herramienta no instalada → no bloquear
    const err = e.stderr ? e.stderr.toString() : (e.stdout ? e.stdout.toString() : String(e));
    process.stderr.write(cmd + ' detectó un error de sintaxis en ' + file + ':\n' + err);
    process.exit(2);
  }
});
