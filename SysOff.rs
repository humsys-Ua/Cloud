// (c) 2026 Oleksandr Anuchin. All rights reserved. 
// PROPRIETARY AND CONFIDENTIAL. Intellectual Property of Oleksandr Anuchin.
// Unauthorized copying, modification, or reverse engineering of this file via any medium is strictly prohibited.

#![no_std]

use core::sync::atomic::{AtomicU8, Ordering};

// --- АПАРАТНІ РЕГІСТРИ ТА ДРАЙВЕРИ (Зовнішні функції керування залізом) ---
extern "C" {
    #[inline(always)]
    pub fn emergency_laser_shutdown();      // Апаратний піновий сигнал (GPIO) на вимкнення драйвера лазера
    pub fn close_steam_valve();             // ШІМ/Реле клапана пари
    pub fn extrude_ion_cassette_to_nest();  // Сервопривод/Пневматика касети
    pub fn open_backup_cooling_valve();     // Клапан резервного контуру чиллера
    pub fn deenergize_ion_grid();           // Вимкнення високовольтного ключа +5 кВ
}

// --- СТАТУСНА МОДЕЛЬ ДЛЯ ПРОМИСЛОВОЇ АВТОМАТИКИ (SCADA / Modbus) ---
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SystemState {
    Operational = 0,
    EmergencyAirKnifeFailure = 1,
    EmergencyMagnetThermalCritical = 2,
}

// Безпечне атомарне сховище для глобального стану системи. 
// Захищає від Data Race між Ядром 1 (200 кГц) та Ядром 2 (Modbus/Slow Loop).
static SYSTEM_STATE: AtomicU8 = AtomicU8::new(SystemState::Operational as u8);

#[inline(always)]
pub fn set_system_state(state: SystemState) {
    // Використовуємо Ordering::SeqCst для гарантії, що всі ядра процесора 
    // миттєво побачать зміну статусу аварії.
    SYSTEM_STATE.store(state as u8, Ordering::SeqCst);
    
    // ТУТ ЛОГІКА ДЛЯ РЕЛЕ: Якщо статус аварійний, можна додатково 
    // "впустити" апаратний сухий контакт на панель оператора.
}

#[inline(always)]
pub fn get_system_state() -> SystemState {
    match SYSTEM_STATE.load(Ordering::SeqCst) {
        1 => SystemState::EmergencyAirKnifeFailure,
        2 => SystemState::EmergencyMagnetThermalCritical,
        _ => SystemState::Operational,
    }
}

// =========================================================================
// 🏗️ НАПРЯМОК 1: ЦЕМЕНТ (Апаратне переривання / Fast Loop)
// =========================================================================

/// Виконується в Fast Loop (200 кГц) або викликається безпосередньо через 
/// апаратне переривання (ISR) від АЦП при падінні тиску.
#[inline(always)]
pub fn check_cement_interlock(diff_pressure_air_knife: f32) {
    // Мінімальний дозволений тиск повітряного ножа в бар
    const CRITICAL_PRESSURE: f32 = 3.5; 
    
    if diff_pressure_air_knife < CRITICAL_PRESSURE {
        // КРИТИЧНЕ ЗАБИВАННЯ ЩІЛИНИ ЦЕМЕНТОМ (Час відгуку < 2 мікросекунди)
        // Порядок викликів критично важливий для захисту дзеркал Фабрі-Перо!
        unsafe {
            emergency_laser_shutdown();      // 1. МИТТЄВО (наносекунди) гасимо лазер 5 кВт на рівні GPIO
            close_steam_valve();             // 2. Припиняємо подачу пари (запобігання утворенню цементної кірки)
            extrude_ion_cassette_to_nest();  // 3. Рятуємо ніхромову сітку від абразиву печі
        }
        
        // Атомарно переводимо систему в глобальний статус аварії
        set_system_state(SystemState::EmergencyAirKnifeFailure);
    }
}

// =========================================================================
// 🏭 НАПРЯМОК 2: МЕТАЛУРГІЯ (Slow Loop 10 Гц з виходом на Fast-переривання)
// =========================================================================

/// Виконується в Slow Loop (10 Гц) для моніторингу теплового стану магнітних кишень NdFeB.
pub fn check_metallurgy_thermal_interlock(magnet_water_temp: f32) {
    const WARNING_TEMP: f32 = 65.0;   // Поріг увімкнення резервного чиллера
    const CRITICAL_TEMP: f32 = 75.0;  // Поріг повної деградації магнітів N52H

    if magnet_water_temp >= WARNING_TEMP && magnet_water_temp < CRITICAL_TEMP {
        // Температура росте: превентивно нарощуємо охолодження
        unsafe { open_backup_cooling_valve(); }
    } else if magnet_water_temp >= CRITICAL_TEMP {
        // РИЗИК БЕЗПОВОРОТНОЇ ВТРАТИ МАГНІТНИХ ВЛАСТИВОСТЕЙ
        unsafe {
            emergency_laser_shutdown();   // 1. Гасимо лазер (прибираємо джерело енергії 50 МВт у резонаторі)
            deenergize_ion_grid();        // 2. Знеструмлюємо сітку (+5 кВ -> 0 В)
        }
        
        set_system_state(SystemState::EmergencyMagnetThermalCritical);
    }
}
