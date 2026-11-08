//! Ajustes para o app gastar menos memória (medições e plano em `docs/MEMORIA.md`).
//!
//! `TELACAST_SEM_AJUSTES=1` desliga tudo, para comparar antes e depois no mesmo binário.

/// Chamar no começo do `main`, antes de qualquer thread e antes de o eframe criar a instância gráfica.
pub fn preparar() {
    if std::env::var_os("TELACAST_SEM_AJUSTES").is_some() {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        vulkan_so_com_drivers_de_gpu();
        // glibc cria uma arena de malloc por thread e quase não devolve memória; com poucas arenas o RSS fica menor
        // e `malloc_trim` consegue devolver páginas.
        // SAFETY: `mallopt` só mexe em parâmetros do alocador e roda antes de existirem outras threads.
        unsafe {
            libc::mallopt(libc::M_ARENA_MAX, 2);
        }
    }
}

/// O carregador do Vulkan abre *todos* os drivers instalados (lavapipe, nouveau, radeon, asahi, virtio…) e vários
/// puxam a libLLVM inteira (~17 MB de RSS) e threads de compilação, sem uso. Mantém só os drivers dos fabricantes de GPU
/// que existem de verdade nesta máquina (lido em `/sys/class/drm`). Não mexe se o usuário já definiu a variável, nem se
/// nenhum driver instalado serve às GPUs encontradas (aí o lavapipe, de software, pode ser o único caminho).
#[cfg(target_os = "linux")]
fn vulkan_so_com_drivers_de_gpu() {
    if std::env::var_os("VK_LOADER_DRIVERS_DISABLE").is_some() || std::env::var_os("VK_LOADER_DRIVERS_SELECT").is_some() {
        return;
    }
    let dirs = ["/usr/share/vulkan/icd.d", "/etc/vulkan/icd.d", "/usr/local/share/vulkan/icd.d"];
    let nomes: Vec<String> = dirs
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()))
        .collect();
    let mut fabricantes: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
        for e in rd.flatten() {
            if let Ok(v) = std::fs::read_to_string(e.path().join("device/vendor")) {
                let v = v.trim().to_lowercase();
                if !fabricantes.contains(&v) {
                    fabricantes.push(v);
                }
            }
        }
    }
    if let Some(valor) = drivers_a_desligar(&nomes, &fabricantes) {
        // SAFETY: ainda só existe a thread principal.
        unsafe { std::env::set_var("VK_LOADER_DRIVERS_DISABLE", valor) };
    }
}

/// `instalados`: nomes dos manifestos de drivers Vulkan; `fabricantes`: ids PCI das GPUs (`0x10de` NVIDIA, `0x1002` AMD,
/// `0x8086` Intel). Devolve o valor de `VK_LOADER_DRIVERS_DISABLE` (nomes exatos separados por vírgula) ou `None`.
pub fn drivers_a_desligar(instalados: &[String], fabricantes: &[String]) -> Option<String> {
    let tem = |f: &str| fabricantes.iter().any(|x| x == f);
    let nvidia_proprietario = instalados.iter().any(|n| n.to_lowercase().starts_with("nvidia"));
    let serve = |n: &str| {
        let n = n.to_lowercase();
        (tem("0x10de") && (n.starts_with("nvidia") || (!nvidia_proprietario && n.starts_with("nouveau"))))
            || (tem("0x1002") && (n.starts_with("radeon") || n.starts_with("amd")))
            || (tem("0x8086") && n.starts_with("intel"))
    };
    if !instalados.iter().any(|n| serve(n)) {
        return None; // nenhum driver de GPU real para as placas desta máquina: não desliga nada
    }
    let fora: Vec<&str> = instalados.iter().filter(|n| !serve(n)).map(String::as_str).collect();
    (!fora.is_empty()).then(|| fora.join(","))
}

/// Configuração do wgpu: só os backends modernos (Vulkan, DX12, Metal). O GL, que o eframe liga por padrão como
/// reserva, carrega a pilha EGL/Mesa (libgallium + LLVM) só para listar adaptadores. `WGPU_BACKEND` do usuário vence.
pub fn wgpu() -> eframe::egui_wgpu::WgpuConfiguration {
    let mut cfg = eframe::egui_wgpu::WgpuConfiguration::default();
    if std::env::var_os("TELACAST_SEM_AJUSTES").is_none() && std::env::var_os("WGPU_BACKEND").is_none() {
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(n) = &mut cfg.wgpu_setup {
            n.instance_descriptor.backends = eframe::wgpu::Backends::PRIMARY;
        }
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn nvidia_mantem_so_o_proprietario() {
        let r = drivers_a_desligar(
            &v(&["lvp_icd.json", "nvidia_icd.json", "nouveau_icd.json", "radeon_icd.json", "asahi_icd.json"]),
            &v(&["0x10de"]),
        )
        .unwrap();
        for fora in ["lvp_icd.json", "nouveau_icd.json", "radeon_icd.json", "asahi_icd.json"] {
            assert!(r.split(',').any(|x| x == fora), "{fora} devia sair: {r}");
        }
        assert!(!r.contains("nvidia_icd"));
    }

    #[test]
    fn nouveau_fica_quando_nao_ha_driver_proprietario() {
        let r = drivers_a_desligar(&v(&["lvp_icd.json", "nouveau_icd.json"]), &v(&["0x10de"])).unwrap();
        assert_eq!(r, "lvp_icd.json");
    }

    #[test]
    fn portatil_com_intel_e_nvidia_mantem_os_dois() {
        let r = drivers_a_desligar(
            &v(&["lvp_icd.json", "nvidia_icd.json", "intel_icd.json", "radeon_icd.json"]),
            &v(&["0x8086", "0x10de"]),
        )
        .unwrap();
        assert_eq!(r.split(',').collect::<Vec<_>>(), vec!["lvp_icd.json", "radeon_icd.json"]);
    }

    #[test]
    fn sem_driver_para_a_placa_nao_mexe() {
        // GPU AMD mas só há lavapipe e nvidia instalados: deixa tudo como está
        assert_eq!(drivers_a_desligar(&v(&["lvp_icd.json", "nvidia_icd.json"]), &v(&["0x1002"])), None);
        assert_eq!(drivers_a_desligar(&v(&["lvp_icd.json"]), &[]), None);
        assert_eq!(drivers_a_desligar(&[], &v(&["0x10de"])), None);
    }

    #[test]
    fn nada_a_desligar_devolve_none() {
        assert_eq!(drivers_a_desligar(&v(&["nvidia_icd.json"]), &v(&["0x10de"])), None);
    }
}
