// La identidad de un monitor, ida y vuelta entre el backend y el frontend.
//
// # Por qué esto vive en su propio archivo
//
// Es la lógica más chica del módulo y la que más caro sale si falla: si un id
// mal formado se "adivinara" en vez de rechazarse, el cambio se le aplicaría al
// monitor equivocado y el usuario vería apagarse una pantalla que no tocó.
//
// Sacarla acá afuera no es prolijidad: este archivo **no toca el crate
// `windows`**, así que es de los pocos que se pueden **testear de verdad** —
// linkear un binario de test que dependa de `windows` falla en la máquina del
// dueño y el CI no corría tests de Rust. Ver `docs/DECISIONS.md`.
//
// # El formato
//
// `"{adapter_luid}:{target_id}:{edid_hash|none}"`, todo en decimal.
//
// Los números van como **texto** porque `adapter_luid` y `edid_hash` son `u64` y
// superan `Number.MAX_SAFE_INTEGER` (2^53): JavaScript los redondearía en
// silencio, justo en los campos que definen la identidad del monitor.
#![cfg(target_os = "windows")]

use monarch::DisplayId;

/// El id que ve el frontend.
///
/// **Tiene que coincidir exactamente** con el que arma `views_from_topology` en
/// `mod.rs`: es la clave con la que el frontend pide el toggle y con la que se
/// busca el monitor al verificar que el cambio haya tomado efecto. Si las dos
/// formas se separan, el toggle empieza a fallar con "ese monitor ya no está en
/// la lista" sin que nada más se rompa.
pub fn format_display_id(id: &DisplayId) -> String {
    format!(
        "{}:{}:{}",
        id.adapter_luid,
        id.target_id,
        id.edid_hash
            .map(|hash| hash.to_string())
            .unwrap_or_else(|| "none".to_string())
    )
}

/// La vuelta. **Estricto a propósito**: ante cualquier duda, error — nunca una
/// interpretación optimista. Ver la cabecera del archivo.
pub fn parse_display_id(raw: &str) -> Result<DisplayId, String> {
    let mut partes = raw.split(':');
    let (Some(luid), Some(target), Some(edid), None) =
        (partes.next(), partes.next(), partes.next(), partes.next())
    else {
        return Err(format!("id de monitor mal formado: {raw}"));
    };

    let adapter_luid = luid
        .parse::<u64>()
        .map_err(|_| format!("id de monitor mal formado (adaptador): {raw}"))?;
    let target_id = target
        .parse::<u32>()
        .map_err(|_| format!("id de monitor mal formado (target): {raw}"))?;
    let edid_hash = match edid {
        "none" => None,
        otro => Some(
            otro.parse::<u64>()
                .map_err(|_| format!("id de monitor mal formado (edid): {raw}"))?,
        ),
    };

    Ok(DisplayId {
        adapter_luid,
        target_id,
        edid_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_id_va_y_vuelve_sin_perder_nada() {
        let original = DisplayId {
            // Más grande de lo que un Number de JS puede representar: si algún
            // día esto viajara como número, este test se pone rojo.
            adapter_luid: (1u64 << 53) + 1,
            target_id: 4242,
            edid_hash: Some(u64::MAX),
        };
        let texto = format_display_id(&original);
        assert_eq!(parse_display_id(&texto), Ok(original));
    }

    #[test]
    fn un_monitor_sin_edid_tambien_va_y_vuelve() {
        let original = DisplayId {
            adapter_luid: 1,
            target_id: 2,
            edid_hash: None,
        };
        assert_eq!(parse_display_id(&format_display_id(&original)), Ok(original));
    }

    #[test]
    fn un_id_basura_se_rechaza_en_vez_de_adivinar() {
        // Adivinar acá le apaga al usuario el monitor equivocado.
        for basura in [
            "",
            "1",
            "1:2",
            "1:2:3:4",
            "a:2:3",
            "1:b:3",
            "1:2:c",
            "1:2:",
            ":2:3",
            "-1:2:3",                      // los u64 no son negativos
            "99999999999999999999:2:3",    // desborda u64
            "1:99999999999:3",             // desborda u32
        ] {
            assert!(
                parse_display_id(basura).is_err(),
                "tendría que rechazar {basura:?}"
            );
        }
    }

    #[test]
    fn el_formato_es_el_que_espera_el_frontend() {
        // El frontend lo usa como `li.dataset.id`. Fijarlo acá para que un
        // cambio de formato rompa un test y no el render por diff.
        assert_eq!(
            format_display_id(&DisplayId {
                adapter_luid: 7,
                target_id: 8,
                edid_hash: Some(9),
            }),
            "7:8:9"
        );
        assert_eq!(
            format_display_id(&DisplayId {
                adapter_luid: 7,
                target_id: 8,
                edid_hash: None,
            }),
            "7:8:none"
        );
    }
}
