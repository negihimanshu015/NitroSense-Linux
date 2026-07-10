// Guard against running outside of a Tauri WebView to prevent TypeError at startup.
if (!window.__TAURI__) {
  console.error('[NitroSense] Tauri IPC is not available. This app must be run inside a Tauri window.');
  document.body.innerHTML = '<div style="display:flex;align-items:center;justify-content:center;height:100vh;color:#fff;font-family:sans-serif;font-size:1.2rem;">Must be launched as a Tauri app.</div>';
  throw new Error('Tauri IPC unavailable');
}

const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;
const appWindow = getCurrentWindow();

let currentMode = 'auto';
let appConfig = null;
let isInitializing = false;

let saveTimeout;
function saveConfig() {
  if (isInitializing) return;
  clearTimeout(saveTimeout);
  saveTimeout = setTimeout(async () => {
    try {
      await invoke('save_config', { config: appConfig });
    } catch (e) {
      console.error('Failed to save config:', e);
    }
  }, 500);
}

const btnAuto = document.getElementById('btn-auto');
const btnMax = document.getElementById('btn-max');
const btnCustom = document.getElementById('btn-custom');
const customControls = document.getElementById('custom-controls');

const cpuSlider = document.getElementById('cpu-slider');
const gpuSlider = document.getElementById('gpu-slider');
const cpuVal = document.getElementById('cpu-val');
const gpuVal = document.getElementById('gpu-val');

async function setMode(mode) {
  try {
    await invoke('set_fan_mode', { mode });
    currentMode = mode;
    
    btnAuto.classList.toggle('active', mode === 'auto');
    btnMax.classList.toggle('active', mode === 'max');
    btnCustom.classList.toggle('active', mode === 'custom');
    
    if (appConfig) {
      appConfig.fan_mode = mode;
      await saveConfig();
    }
    
    const isCustom = mode === 'custom';
    customControls.classList.toggle('disabled', !isCustom);
    cpuSlider.disabled = !isCustom;
    gpuSlider.disabled = !isCustom;
  } catch (error) {
    console.error('Failed to set fan mode:', error);
    alert('Failed to set fan mode. Make sure you have permissions to write to /proc/acpi/call.');
  }
}

async function applyCustomSpeeds() {
  if (currentMode !== 'custom') return;
  
  const cpuPercent = parseInt(cpuSlider.value);
  const gpuPercent = parseInt(gpuSlider.value);
  
  try {
    await invoke('set_fan_speed', { cpuPercent, gpuPercent });
    if (appConfig) {
      appConfig.cpu_percent = cpuPercent;
      appConfig.gpu_percent = gpuPercent;
      await saveConfig();
    }
  } catch (error) {
    console.error('Failed to apply custom speeds:', error);
  }
}

btnAuto.addEventListener('click', () => setMode('auto'));
btnMax.addEventListener('click', () => setMode('max'));
btnCustom.addEventListener('click', () => setMode('custom'));

let applyTimeout;
function handleSliderInput(e, isCpu) {
  if (isCpu) {
    cpuVal.innerText = `${e.target.value}%`;
  } else {
    gpuVal.innerText = `${e.target.value}%`;
  }
  
  clearTimeout(applyTimeout);
  applyTimeout = setTimeout(applyCustomSpeeds, 150);
}

cpuSlider.addEventListener('input', (e) => handleSliderInput(e, true));
gpuSlider.addEventListener('input', (e) => handleSliderInput(e, false));


document.getElementById('win-minimize').addEventListener('click', () => appWindow.minimize());
document.getElementById('win-close').addEventListener('click', () => appWindow.close());

const cpuHistory = [];
const gpuHistory = [];
const MAX_POINTS = 30;

function updateChartPath(elementId, history) {
  const pathEl = document.getElementById(elementId);
  if (!pathEl || history.length === 0) return;
  
  const width = 500;
  const height = 100;
  const step = width / (MAX_POINTS - 1);
  
  let d = '';
  for (let i = 0; i < history.length; i++) {
    const x = i * step;
    const temp = Math.min(Math.max(history[i], 0), 100);
    const y = height - temp;
    
    if (i === 0) {
      d += `M ${x} ${y}`;
    } else {
      d += ` L ${x} ${y}`;
    }
  }
  pathEl.setAttribute('d', d);
}

// Telemetry Polling Loop
async function updateTelemetry() {
  try {
    let cpuTemp = 0, gpuTemp = 0, cpuRpm = 0, gpuRpm = 0;
    try {
      const data = await invoke('get_telemetry');
      [cpuTemp, gpuTemp, cpuRpm, gpuRpm] = data;
    } catch (e) {
      console.warn('ACPI Telemetry error:', e);
    }
    
    // Update text elements
    document.querySelectorAll('.text-cpu-temp').forEach(el => el.innerText = `${cpuTemp}°C`);
    document.querySelectorAll('.text-gpu-temp').forEach(el => el.innerText = `${gpuTemp}°C`);
    document.querySelectorAll('.text-cpu-rpm').forEach(el => el.innerText = cpuRpm);
    document.querySelectorAll('.text-gpu-rpm').forEach(el => el.innerText = gpuRpm);
    
    // Calculate SVG stroke-dashoffset using computed circumference.
    const CIRC = 2 * Math.PI * 42;

    // Clamp RPM ring fill to hardware limit (6000 RPM).
    const MAX_RPM = 6000;

    const cpuRing = document.getElementById('gov-cpu-temp-ring');
    const gpuRing = document.getElementById('gov-gpu-temp-ring');
    if (cpuRing) cpuRing.style.strokeDashoffset = CIRC - ((Math.min(cpuTemp, 100) / 100) * CIRC);
    if (gpuRing) gpuRing.style.strokeDashoffset = CIRC - ((Math.min(gpuTemp, 100) / 100) * CIRC);

    const cpuRpmRing = document.getElementById('gov-cpu-rpm-ring');
    const gpuRpmRing = document.getElementById('gov-gpu-rpm-ring');
    if (cpuRpmRing) cpuRpmRing.style.strokeDashoffset = CIRC - ((Math.min(cpuRpm, MAX_RPM) / MAX_RPM) * CIRC);
    if (gpuRpmRing) gpuRpmRing.style.strokeDashoffset = CIRC - ((Math.min(gpuRpm, MAX_RPM) / MAX_RPM) * CIRC);

    // Update System Status (sysinfo & nvidia-smi)
    try {
      const [cpuUsage, ramUsage, gpuUsage] = await invoke('get_system_status');
      
      const cpuUsageEls = document.querySelectorAll('.val-cpu-util');
      const cpuUsageBars = document.querySelectorAll('#bar-cpu-util');
      cpuUsageEls.forEach(el => el.innerText = `${cpuUsage.toFixed(1)}%`);
      cpuUsageBars.forEach(el => el.style.width = `${cpuUsage}%`);

      const ramUsageEls = document.querySelectorAll('.val-ram-util');
      const ramUsageBars = document.querySelectorAll('#bar-ram-util');
      ramUsageEls.forEach(el => el.innerText = `${ramUsage.toFixed(1)}%`);
      ramUsageBars.forEach(el => el.style.width = `${ramUsage}%`);
      
      const gpuUsageEls = document.querySelectorAll('.val-gpu-util');
      const gpuUsageBars = document.querySelectorAll('#bar-gpu-util');
      gpuUsageEls.forEach(el => el.innerText = `${gpuUsage.toFixed(1)}%`);
      gpuUsageBars.forEach(el => el.style.width = `${gpuUsage}%`);
    } catch (e) {
      console.warn('System status error:', e);
    }

    // Update History Chart
    cpuHistory.push(cpuTemp);
    gpuHistory.push(gpuTemp);
    if (cpuHistory.length > MAX_POINTS) cpuHistory.shift();
    if (gpuHistory.length > MAX_POINTS) gpuHistory.shift();
    updateChartPath('path-cpu', cpuHistory);
    updateChartPath('path-gpu', gpuHistory);

  } catch (error) {
    console.error('Failed to update telemetry loop:', error);
  }
}

let telemetryIntervalId;

window.addEventListener('unload', () => {
  if (telemetryIntervalId) {
    clearInterval(telemetryIntervalId);
  }
});

// Check if /proc/acpi/call is writable and WMI responds to probe.
async function checkDeps() {
  const [acpiOk, wmiOk] = await invoke('check_dependencies');
  if (!acpiOk || !wmiOk) {
    const headline = !acpiOk
      ? 'Missing: acpi_call Module'
      : 'Missing: Acer WMI Interface';
    const detail = !acpiOk
      ? 'The <code>acpi_call</code> kernel module is not loaded or <code>/proc/acpi/call</code> is not writable. Run the permissions installer to fix this.'
      : 'The <code>acpi_call</code> file is accessible, but the Acer WMID WMI device path (<code>_SB.PC00.WMID.WMBH</code>) did not respond. Make sure <code>acer_wmi</code> is loaded and your hardware is supported.';

    const overlay = document.createElement('div');
    overlay.style.position = 'fixed';
    overlay.style.top = '0'; overlay.style.left = '0';
    overlay.style.width = '100vw'; overlay.style.height = '100vh';
    overlay.style.backgroundColor = 'rgba(10, 10, 15, 0.95)';
    overlay.style.backdropFilter = 'blur(10px)';
    overlay.style.zIndex = '9999';
    overlay.style.display = 'flex';
    overlay.style.flexDirection = 'column';
    overlay.style.justifyContent = 'center';
    overlay.style.alignItems = 'center';
    overlay.style.color = 'white';
    overlay.style.fontFamily = 'Outfit, sans-serif';
    overlay.style.padding = '2rem';
    overlay.style.textAlign = 'center';

    overlay.innerHTML = `
      <h1 style="color: #ff3366; font-size: 2.5rem; margin-bottom: 1rem;">${headline}</h1>
      <p style="font-size: 1.2rem; color: #a0a0b0; max-width: 600px; line-height: 1.6;">${detail}</p>
      <div style="background: rgba(255,255,255,0.05); padding: 1.5rem; border-radius: 12px; margin-top: 2rem; border: 1px solid rgba(255,255,255,0.1); text-align: left;">
        <h3 style="margin-top: 0; color: #fff;">How to fix:</h3>
        <ol style="color: #a0a0b0; margin-bottom: 0; padding-left: 1.2rem; line-height: 1.8;">
          <li>Open your terminal in the <code>nitrosense-linux</code> folder.</li>
          <li>Run: <code>sudo ./install-permissions.sh</code></li>
          <li>Log out and back in (or run <code>newgrp nitrosense</code>).</li>
          <li>Restart this application.</li>
        </ol>
      </div>
    `;
    document.body.appendChild(overlay);
    return false;
  }
  return true;
}

// Navigation & RGB tab switching (supporting keyboard accessibility)
function activateNavItem(item) {
  document.querySelectorAll('.nav-item').forEach(nav => nav.classList.remove('active'));
  item.classList.add('active');
  document.querySelectorAll('.screen-panel').forEach(panel => panel.classList.remove('active'));
  const target = document.getElementById(item.getAttribute('data-target'));
  if (target) target.classList.add('active');
}

document.querySelectorAll('.nav-item').forEach(item => {
  item.addEventListener('click', () => activateNavItem(item));
  item.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activateNavItem(item);
    }
  });
});

let activeZone = 1;
let currentRgbMode = 0;
let currentRgbSpeed = 2;
let currentBrightness = 80;
function parseColor(colorStr) {
    if (!colorStr) return null;
    let match = colorStr.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/);
    if (match) {
        return { r: parseInt(match[1]), g: parseInt(match[2]), b: parseInt(match[3]) };
    }
    match = colorStr.match(/#([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})/i);
    if (match) {
        return {
            r: parseInt(match[1], 16),
            g: parseInt(match[2], 16),
            b: parseInt(match[3], 16)
        };
    }
    return null;
}

const zoneElements = document.querySelectorAll('.kb-zone');
function activateZone(z) {
  zoneElements.forEach(el => el.classList.remove('selected'));
  z.classList.add('selected');
  activeZone = parseInt(z.getAttribute('data-zone'));
  document.querySelector('.editor-title').innerText = `Zone ${activeZone} Illumination`;
  const currentGlow = z.style.getPropertyValue('--zone-glow') || window.getComputedStyle(z).getPropertyValue('--zone-glow').trim();
  if (currentGlow) {
      const parsedGlow = parseColor(currentGlow);
      if (parsedGlow) {
          paletteColors.forEach(el => {
              const elColor = window.getComputedStyle(el).backgroundColor;
              const parsedEl = parseColor(elColor);
              if (parsedEl && parsedEl.r === parsedGlow.r && parsedEl.g === parsedGlow.g && parsedEl.b === parsedGlow.b) {
                  paletteColors.forEach(c => c.classList.remove('selected'));
                  el.classList.add('selected');
              }
          });
      }
  }
}

zoneElements.forEach(z => {
  z.addEventListener('click', () => activateZone(z));
  z.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activateZone(z);
    }
  });
});

const paletteColors = document.querySelectorAll('.palette-color');
async function activatePaletteColor(colorEl) {
  paletteColors.forEach(el => el.classList.remove('selected'));
  colorEl.classList.add('selected');

  const bg = window.getComputedStyle(colorEl).backgroundColor;
  const match = bg.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/);
  if (match) {
    const r = parseInt(match[1]);
    const g = parseInt(match[2]);
    const b = parseInt(match[3]);

    try {
      await invoke('set_rgb_zone', { zone: activeZone, r, g, b });

      // Dynamically update the active zone's glow color in UI
      const activeZoneEl = document.querySelector(`.kb-zone-${activeZone}`);
      if (activeZoneEl) {
        activeZoneEl.style.setProperty('--zone-glow', bg);
      }
      if (appConfig) {
        appConfig[`rgb_zone_color_${activeZone}`] = bg;
        await saveConfig();
      }

      if (currentRgbMode === 0) {
        await applyRgb();
      }
    } catch (e) {
      console.error("Failed to set color:", e);
    }
  }
}

paletteColors.forEach(colorEl => {
  colorEl.addEventListener('click', () => activatePaletteColor(colorEl));
  colorEl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activatePaletteColor(colorEl);
    }
  });
});

const presetBtns = document.querySelectorAll('.preset-btn');
presetBtns.forEach(btn => {
  btn.addEventListener('click', async () => {
    presetBtns.forEach(el => el.classList.remove('active'));
    btn.classList.add('active');
    const preset = btn.getAttribute('data-preset');

    if (preset === 'static') currentRgbMode = 0;
    else if (preset === 'breathing') currentRgbMode = 1;
    else if (preset === 'neon') currentRgbMode = 2;
    else if (preset === 'wave') currentRgbMode = 3;

    if (appConfig) {
      appConfig.rgb_mode = preset;
      await saveConfig();
    }

    const speedGroup = document.querySelector('.slider-speed').closest('.slider-group');
    const isStatic = (preset === 'static');
    speedGroup.classList.toggle('disabled', isStatic);
    document.querySelector('.slider-speed').disabled = isStatic;

    await applyRgb();
  });
});


function speedLabel(val) {
    // Clamp speed index to prevent invalid text in UI.
    if (val === 1) return 'Slow';
    if (val === 3) return 'Fast';
    return 'Medium';
}

const sliderBrightness = document.querySelector('.slider-brightness');
const valBrightness = document.querySelector('.val-brightness');
sliderBrightness.addEventListener('change', async (e) => {
    currentBrightness = parseInt(e.target.value);
    if (appConfig) {
      appConfig.rgb_brightness = currentBrightness;
      await saveConfig();
    }
    await applyRgb();
});
sliderBrightness.addEventListener('input', (e) => valBrightness.innerText = `${e.target.value}%`);

const sliderSpeed = document.querySelector('.slider-speed');
const valSpeed = document.querySelector('.val-speed');
sliderSpeed.addEventListener('change', async (e) => {
    const val = parseInt(e.target.value);
    currentRgbSpeed = val;
    if (appConfig) {
      appConfig.rgb_speed_index = val;
      await saveConfig();
    }
    if (currentRgbMode !== 0) await applyRgb();
});
sliderSpeed.addEventListener('input', (e) => {
    valSpeed.innerText = speedLabel(parseInt(e.target.value));
});

async function applyRgb() {
    try {
        await invoke('apply_rgb_settings', { 
            mode: currentRgbMode, 
            speed: currentRgbMode === 0 ? 0 : currentRgbSpeed, 
            brightness: currentBrightness 
        });
    } catch (e) {
        console.error("Failed to apply RGB:", e);
    }
}

async function initApp() {
    isInitializing = true;
    try {
        try {
            await invoke('init_rgb');
        } catch (e) {
            console.error("Failed to initialize RGB hardware:", e);
        }

        try {
            appConfig = await invoke('load_config');
        } catch (e) {
            console.error("Failed to load config, using fallback defaults:", e);
            appConfig = {
                fan_mode: "auto",
                cpu_percent: 50,
                gpu_percent: 50,
                rgb_mode: "static",
                rgb_brightness: 80,
                rgb_speed_index: 2,
                rgb_zone_color_1: "rgb(255, 0, 0)",
                rgb_zone_color_2: "rgb(0, 255, 0)",
                rgb_zone_color_3: "rgb(0, 0, 255)",
                rgb_zone_color_4: "rgb(255, 255, 0)"
            };
        }

        // Restore Fan State
        cpuSlider.value = appConfig.cpu_percent;
        cpuVal.innerText = `${appConfig.cpu_percent}%`;
        gpuSlider.value = appConfig.gpu_percent;
        gpuVal.innerText = `${appConfig.gpu_percent}%`;

        await setMode(appConfig.fan_mode);
        if (appConfig.fan_mode === 'custom') {
            await applyCustomSpeeds();
        }

        // Restore RGB State
        for (let z = 1; z <= 4; z++) {
            const savedColor = appConfig[`rgb_zone_color_${z}`];
            const parsed = parseColor(savedColor);
            if (parsed) {
                const { r, g, b } = parsed;
                try {
                    await invoke('set_rgb_zone', { zone: z, r, g, b });
                    const zoneEl = document.querySelector(`.kb-zone-${z}`);
                    if (zoneEl) {
                        zoneEl.style.setProperty('--zone-glow', savedColor);
                    }
                } catch (e) {
                    console.error(`Failed to initialize zone ${z}:`, e);
                }
            }
        }

        currentBrightness = appConfig.rgb_brightness;
        sliderBrightness.value = currentBrightness;
        valBrightness.innerText = `${currentBrightness}%`;

        sliderSpeed.value = appConfig.rgb_speed_index;
        valSpeed.innerText = speedLabel(appConfig.rgb_speed_index);
        currentRgbSpeed = appConfig.rgb_speed_index;

        const savedMode = appConfig.rgb_mode;
        let clicked = false;
        if (savedMode) {
            const targetBtn = document.querySelector(`.preset-btn[data-preset="${savedMode}"]`);
            if (targetBtn) {
                targetBtn.click();
                clicked = true;
            }
        } else {
            const activePreset = document.querySelector('.preset-btn.active');
            if (activePreset && activePreset.getAttribute('data-preset') === 'static') {
                const initSpeedGroup = document.querySelector('.slider-speed').closest('.slider-group');
                initSpeedGroup.classList.add('disabled');
                document.querySelector('.slider-speed').disabled = true;
            }
        }

        if (!clicked) {
            await applyRgb();
        }
        
        const defaultZoneEl = document.querySelector(`.kb-zone-${activeZone}`);
        if (defaultZoneEl) {
            defaultZoneEl.click();
        }
    } finally {
        isInitializing = false;
    }
}

async function startApp() {
    const depsOk = await checkDeps();
    if (!depsOk) return;

    await initApp();

    // Start telemetry polling only after initialization is complete
    updateTelemetry();
    telemetryIntervalId = setInterval(updateTelemetry, 2000);
}

startApp().catch(console.error);
