// Persistencia de la config de displays — Fase 2 del SPEC-displays.
//
// El `MonarchDisplayManager` del crate puro pide un `ConfigStore` para guardar
// perfiles, huellas de monitores y el `last_known_good_layout` (la red del
// auto-rollback). El crate trae un `FileConfigStore` que NO se usa acá por dos
// razones, y la primera es grave:
//
//  1. Su `default_config_path()` apunta a `%APPDATA%\Monarch\config.json` — la
//     configuración REAL de Monarch del usuario. Millennium escribiendo ahí le
//     pisaría los perfiles a la otra app. Por eso este archivo jamás nombra a
//     `FileConfigStore`, y hay un test que falla si alguien re-cablea la ruta.
//  2. Escribe con `fs::write` directo: si el proceso muere a mitad de camino
//     queda un JSON truncado. Millennium ya arregló esa clase de bug con
//     `json_store::JsonStore` (tmp + rename atómico, backup del archivo
//     corrupto antes de caer al default). Se reusa eso.
//
// El archivo es windows-only como el resto del módulo, PERO a propósito no
// toca el crate `windows` ni `tauri`: solo `monarch`, `crate::json_store`,
// `std` y `serde`. Así se puede type-checkear y **correr los tests** en el
// crate scratch que no linkea contra `windows` (en esta máquina linkear
// `windows` falla por falta de `dlltool.exe` — ver docs/DECISIONS.md).
//
// Tampoco loguea: `diagnostics::log` cuelga de `crate::runtime_log`, que es
// justo lo que rompería esa independencia. No hace falta — `JsonStore` ya
// escribe el ERR cuando el archivo estaba corrupto, y para lo demás está
// `loaded_from_corrupt()`, que el llamador consulta y reporta.
#![cfg(target_os = "windows")]

use std::path::{Path, PathBuf};

use monarch::{AppConfig, ConfigStore, ManagerError};

use crate::json_store::JsonStore;

/// Carpeta de datos de Millennium dentro de `%APPDATA%`. Es el identificador
/// del bundle de Tauri; tiene que coincidir con lo que devuelve
/// `app.path().app_data_dir()`, que es de donde sale el `&Path` real.
///
/// En producción no se usa (la ruta la trae Tauri ya resuelta): existe para que
/// el test de la trampa pueda afirmar dónde NO tiene que caer el archivo. Ese
/// test es la única razón de que este bloque siga acá, y alcanza.
#[allow(dead_code)]
pub const DATA_DIR_NAME: &str = "com.guidocameraeq.millennium";

/// Nombre del archivo, partido como lo pide `JsonStore::load`.
const FILE_BASE: &str = "displays";
const FILE_EXT: &str = "json";

/// `<appdata>/com.guidocameraeq.millennium`.
///
/// Separado de `fallback_data_dir()` para poder testear la resolución sin
/// depender de que la máquina tenga `%APPDATA%` seteado.
#[allow(dead_code)] // ídem DATA_DIR_NAME: lo ejerce el test de la trampa
pub fn data_dir_under(appdata: &Path) -> PathBuf {
    appdata.join(DATA_DIR_NAME)
}

/// Resuelve el data dir desde `%APPDATA%`, para cuando no hay Tauri a mano.
///
/// El camino normal es que `setup()` pase el `&Path` que ya tiene de Tauri;
/// esto es el plan B (tests, herramientas sueltas, código que corre antes de
/// que exista la app). Mismo criterio que `install_panic_hook()` en `lib.rs`.
#[allow(dead_code)] // ídem: el camino de producción recibe el dir de Tauri
pub fn fallback_data_dir() -> Result<PathBuf, ManagerError> {
    match std::env::var_os("APPDATA") {
        Some(appdata) => Ok(data_dir_under(Path::new(&appdata))),
        None => Err(ManagerError::Backend(
            "no se pudo resolver %APPDATA% para guardar la config de displays".to_string(),
        )),
    }
}

/// `ConfigStore` de Monarch respaldado por el store atómico de Millennium.
pub struct MillenniumConfigStore {
    store: JsonStore<AppConfig>,
    data_dir: PathBuf,
}

impl MillenniumConfigStore {
    /// Abre (o crea al primer `save`) `<data_dir>/displays.json`.
    ///
    /// El `data_dir` entra desde afuera —lo tiene `setup()` por Tauri— en vez
    /// de resolverse acá adentro: es la única forma de garantizar que este
    /// store escriba en la carpeta de Millennium y en ninguna otra.
    pub fn new(data_dir: &Path) -> Result<Self, ManagerError> {
        let store = JsonStore::<AppConfig>::load(data_dir, FILE_BASE, FILE_EXT).map_err(|e| {
            // `ManagerError` no tiene variante para `anyhow`; el `{e:#}` trae
            // la cadena de contexto entera, que es donde está la ruta.
            ManagerError::Backend(format!(
                "no se pudo abrir la config de displays en {}: {e:#}",
                data_dir.display()
            ))
        })?;
        Ok(Self {
            store,
            data_dir: data_dir.to_path_buf(),
        })
    }

    // No hay un `from_appdata()` que resuelva la ruta por su cuenta, y es
    // deliberado: cuantas menos formas existan de construir este store, menos
    // formas hay de que una de ellas termine apuntando a otra carpeta. El
    // `data_dir` entra siempre desde `setup()`, que lo saca de Tauri.

    /// Ruta del archivo, para loguear y para el test que vigila que esto no
    /// apunte nunca al `%APPDATA%\Monarch` del usuario.
    ///
    /// Es **informativa**: `JsonStore` le agrega un sufijo al nombre cuando
    /// está seteado `MILLENNIUM_INSTANCE` (el doble arranque de desarrollo),
    /// y eso no se replica acá.
    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join(format!("{FILE_BASE}.{FILE_EXT}"))
    }

    /// `true` si el archivo existía pero no parseaba: el estado en memoria es
    /// el default, no los datos del usuario. Quien vaya a tomar una decisión
    /// destructiva a partir de la config (pisar perfiles, por ejemplo) tiene
    /// que frenar acá. El backup ya lo hizo `JsonStore`.
    pub fn loaded_from_corrupt(&self) -> bool {
        self.store.loaded_from_corrupt()
    }
}

impl ConfigStore for MillenniumConfigStore {
    fn load(&self) -> Result<AppConfig, ManagerError> {
        // `JsonStore` leyó el disco una sola vez, al construirse, y es el
        // único dueño del archivo; la copia en memoria es la verdad.
        Ok(self.store.read(|config| config.clone()))
    }

    fn save(&self, config: &AppConfig) -> Result<(), ManagerError> {
        self.store
            .update(|slot| {
                *slot = config.clone();
            })
            .map_err(|e| {
                ManagerError::Backend(format!(
                    "no se pudo guardar la config de displays en {}: {e:#}",
                    self.config_path().display()
                ))
            })
    }
}

// Los tests no usan `unwrap`/`expect`: devuelven `Result` y encadenan con `?`.
// Es la misma disciplina que el código de producción, sin ruido.
#[cfg(test)]
mod tests {
    use super::*;
    use monarch::{DisplayId, Layout, OutputConfig, Position, Profile, Resolution};
    use std::fs;

    /// Carpeta descartable bajo el temp del sistema. La unicidad sale del pid
    /// más una etiqueta por test — sin crates externos y sin reloj.
    fn scratch(tag: &str) -> Result<PathBuf, ManagerError> {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "millennium-displays-store-{}-{}",
            std::process::id(),
            tag
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// LA RED CONTRA LA TRAMPA. Si alguien re-cablea este store al
    /// `FileConfigStore` del crate puro, la ruta pasa a ser
    /// `%APPDATA%\Monarch\config.json` y este test se pone rojo antes de que
    /// nadie le pise la configuración real de Monarch al usuario.
    #[test]
    fn la_ruta_cae_en_millennium_y_nunca_en_monarch() -> Result<(), ManagerError> {
        let appdata_falso = Path::new("C:\\Users\\test\\AppData\\Roaming");
        let store = MillenniumConfigStore::new(&data_dir_under(appdata_falso))?;

        let ruta = store.config_path().to_string_lossy().to_lowercase();
        assert!(
            ruta.contains(DATA_DIR_NAME),
            "la config tiene que vivir en la carpeta de Millennium: {ruta}"
        );
        assert!(
            ruta.ends_with("displays.json"),
            "el archivo tiene que ser displays.json: {ruta}"
        );
        assert!(
            !ruta.contains("monarch"),
            "PELIGRO: la ruta toca Monarch — eso es la config real del usuario: {ruta}"
        );

        // Y lo mismo para la resolución de verdad, cuando la máquina tiene
        // %APPDATA% (o sea: siempre en Windows).
        if let Ok(real) = fallback_data_dir() {
            let real = real.to_string_lossy().to_lowercase();
            assert!(real.contains(DATA_DIR_NAME), "{real}");
            assert!(!real.contains("monarch"), "PELIGRO: {real}");
        }
        Ok(())
    }

    #[test]
    fn round_trip_guarda_y_relee_un_perfil() -> Result<(), ManagerError> {
        let dir = scratch("roundtrip")?;

        let perfil = Profile {
            name: "TV apagada".to_string(),
            layout: Layout {
                outputs: vec![OutputConfig {
                    display_id: DisplayId {
                        adapter_luid: 1,
                        target_id: 1,
                        edid_hash: Some(1),
                    },
                    enabled: true,
                    position: Position { x: 0, y: 0 },
                    resolution: Resolution {
                        width: 1920,
                        height: 1080,
                    },
                    refresh_rate_mhz: 60_000,
                    primary: true,
                }],
            },
        };
        let config = AppConfig {
            profiles: vec![perfil.clone()],
            ..AppConfig::default()
        };

        {
            let store = MillenniumConfigStore::new(&dir)?;
            store.save(&config)?;
        }

        // Store nuevo sobre la misma carpeta: esto lee el disco de verdad, no
        // la copia en memoria del anterior.
        let store = MillenniumConfigStore::new(&dir)?;
        assert!(
            !store.loaded_from_corrupt(),
            "el archivo recién escrito tiene que parsear"
        );
        assert_eq!(store.load()?.profiles, vec![perfil]);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
