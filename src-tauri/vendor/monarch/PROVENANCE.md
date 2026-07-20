# Origen de este código (vendor)

Este directorio es una **copia** del crate puro `monarch` — la lógica de perfiles,
layout y confirmación de Monarch, sin nada de Windows ni de Tauri (sus únicas
dependencias son `serde` y `serde_json`).

| | |
|---|---|
| **Repo donante** | `guidocameraeq/Monarch` — el **fork de Guido**, NO el upstream `Nuzair46/Monarch` |
| **Commit exacto** | `7f9f63ba59a022f296c94ac85ff0a41adfce0324` (`7f9f63b`, 2026-07-16) |
| **Qué se copió** | `Cargo.toml`, `LICENSE`, `src/{lib,backend,error,manager,model,store}.rs` |
| **Modificaciones** | ninguna — es copia byte a byte del donante |
| **Licencia** | MIT. El `LICENSE` de al lado se preserva íntegro, con **sus dos** líneas de copyright (Nuzair46 por el upstream + Guido Camera por el fork). No borrar ninguna. |

## Por qué copia y no `git subtree`

El crate puro vive en la **raíz** del repo Monarch (`Cargo.toml` arriba, fuentes en
`src/`), mezclado con el resto del proyecto: `src-tauri/`, `web/`, `docs/`,
`package.json`, sus propios workflows de CI. Un `git subtree add` habría traído los
**80 archivos** del repo para usar 8. Un `git subtree split --prefix=src` deja afuera
el `Cargo.toml` de la raíz, así que ni siquiera sería un crate válido.

La trazabilidad que aportaba el subtree la da el commit anotado arriba. Solo 10 de
los 51 commits de Monarch tocaron `src/`, así que re-sincronizar a mano es barato.

Ver `docs/DECISIONS.md` (ADR-001) para la decisión completa.

## Cómo re-sincronizar si Monarch cambia

```sh
# desde la raíz del hub (Millenium y monarch/)
cp Monarch/Cargo.toml Monarch/LICENSE Millennium-Clipboard/src-tauri/vendor/monarch/
cp Monarch/src/*.rs                     Millennium-Clipboard/src-tauri/vendor/monarch/src/
cd Millennium-Clipboard/src-tauri/vendor/monarch && cargo test   # deben dar 22 verdes
```

Después actualizar el commit de esta tabla y el de `docs/DECISIONS.md`.

## Qué usa la Fase 1 de acá

Solo los **tipos del modelo** (`DisplayId`, `DisplayInfo`, `Layout`, `OutputConfig`,
`Resolution`, `Position`) y `ManagerError`. El `MonarchDisplayManager` y el `store`
entran recién en las Fases 2/3 (apply con red de seguridad, perfiles).

⚠️ `MonarchDisplayManager::new()` **no es read-only**: sincroniza huellas y puede
escribir el store. Y con el `FileConfigStore` por default esa escritura va a
`%APPDATA%\Monarch\config.json` — **el config real de Monarch del usuario**. La Fase 1
no lo instancia; cuando la Fase 2 lo haga, tiene que apuntarlo al APPDATA de
Millennium.
