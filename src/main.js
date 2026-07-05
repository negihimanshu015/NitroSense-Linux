// Guard against running outside of a Tauri WebView (e.g. browser dev preview).
// Without this check, accessing window.__TAURI__ when Tauri IPC is unavailable
// throws a TypeError at startup and breaks the entire script.
if (!window.__TAURI__) {
  console.error('[NitroSense] Tauri IPC is not available. This app must be run inside a Tauri window.');
  document.body.innerHTML = '<div style="display:flex;align-items:center;justify-content:center;height:100vh;color:#fff;font-family:sans-serif;font-size:1.2rem;">Must be launched as a Tauri app.</div>';
  throw new Error('Tauri IPC unavailable');
}

const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;
const appWindow = getCurrentWindow();

let currentMode = 'auto';

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
    
    // Update UI
    btnAuto.classList.toggle('active', mode === 'auto');
    btnMax.classList.toggle('active', mode === 'max');
    btnCustom.classList.toggle('active', mode === 'custom');
    
    // Save to localStorage
    localStorage.setItem('fanMode', mode);
    
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
    localStorage.setItem('cpuPercent', cpuPercent);
    localStorage.setItem('gpuPercent', gpuPercent);
  } catch (error) {
    console.error('Failed to apply custom speeds:', error);
  }
}

// Event Listeners
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
cpuSlider.addEventListener('change', applyCustomSpeeds);
gpuSlider.addEventListener('change', applyCustomSpeeds);

// Restore saved states on startup
async function restoreSavedState() {
  const savedCpuPercent = localStorage.getItem('cpuPercent');
  const savedGpuPercent = localStorage.getItem('gpuPercent');
  
  if (savedCpuPercent) {
    cpuSlider.value = savedCpuPercent;
    cpuVal.innerText = `${savedCpuPercent}%`;
  }
  if (savedGpuPercent) {
    gpuSlider.value = savedGpuPercent;
    gpuVal.innerText = `${savedGpuPercent}%`;
  }

  const savedMode = localStorage.getItem('fanMode');
  if (savedMode) {
    await setMode(savedMode);
    
    if (savedMode === 'custom') {
      await applyCustomSpeeds();
    }
  } else {
    // Default to auto if no saved state
    await setMode('auto');
  }
}
restoreSavedState();

// Window Controls Event Listeners
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
    
    // Update SVG rings:
    // stroke-dashoffset=CIRC means fully hidden (empty ring at 0);
    // stroke-dashoffset=0 means fully visible ring (full at max).
    // So offset = CIRC - ((value / max) * CIRC).
    //
    // Exact circumference for r=42: 2 * Math.PI * 42 ≈ 263.8938
    // Using the computed value avoids the ~0.006px gap at full fill that a
    // hardcoded 263.9 would produce.
    const CIRC = 2 * Math.PI * 42;

    // Max observed RPM on Acer Nitro hardware. Fan curves above this value
    // are clamped so the ring fill never visually overflows.
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

// Start polling every 2 seconds.
// Store the interval ID so we can clear it when the window is destroyed.
// Without this, a recreated WebView would accumulate multiple concurrent intervals.
let telemetryIntervalId = setInterval(updateTelemetry, 2000);
updateTelemetry();

window.addEventListener('unload', () => {
  clearInterval(telemetryIntervalId);
});

// Dependency Check
// check_dependencies returns [acpi_ok, wmi_ok]:
//   acpi_ok — /proc/acpi/call is writable (acpi_call loaded + permissions set)
//   wmi_ok  — the Acer WMID WMI path responds to a real probe call
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
  }
}
checkDeps();

// ==========================================
// NAVIGATION & RGB LOGIC
// ==========================================

// Nav Tab Switching
// Uses <li> elements with role="button" and tabindex="0" (set in HTML).
// Both click and keydown (Enter/Space) are handled so keyboard users can navigate.
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

// RGB State
let activeZone = 1;
let currentRgbMode = 0;
let currentRgbSpeed = 5;
let currentBrightness = 80;

// Initialize WMI RGB
invoke('init_rgb').catch(console.error);

// Parse colors to r, g, b components
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

// Zone selector
// .kb-zone elements have role="button" and tabindex="0" (set in HTML).
const zoneElements = document.querySelectorAll('.kb-zone');
function activateZone(z) {
  zoneElements.forEach(el => el.classList.remove('selected'));
  z.classList.add('selected');
  activeZone = parseInt(z.getAttribute('data-zone'));
  document.querySelector('.editor-title').innerText = `Zone ${activeZone} Illumination`;

  // Highlight the active color for this zone in the palette
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

// Color Palette
// .palette-color elements have role="button" and tabindex="0" (set in HTML).
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
      // Save to localStorage to persist state across refreshes
      localStorage.setItem(`rgbZoneColor_${activeZone}`, bg);

      if (currentRgbMode === 0) { // Re-apply static settings to push to hardware
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

// Preset buttons
// Preset buttons are <button> elements so Enter/Space are handled natively by the browser.
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

    localStorage.setItem('rgbMode', preset);

    // Disable speed slider for static color
    const speedGroup = document.querySelector('.slider-speed').closest('.slider-group');
    const isStatic = (preset === 'static');
    speedGroup.classList.toggle('disabled', isStatic);
    document.querySelector('.slider-speed').disabled = isStatic;

    await applyRgb();
  });
});

// Speed helpers
function speedFromIndex(val) {
    if (val === 1) return 1;
    if (val === 3) return 9;
    return 5;
}
function speedLabel(val) {
    // Clamp out-of-range values (e.g. a corrupted localStorage entry of 0 or 4)
    // so we never render "undefined" in the UI. 1=Slow, 3=Fast, anything else=Medium.
    if (val === 1) return 'Slow';
    if (val === 3) return 'Fast';
    return 'Medium';
}

// Sliders
const sliderBrightness = document.querySelector('.slider-brightness');
const valBrightness = document.querySelector('.val-brightness');
sliderBrightness.addEventListener('change', async (e) => {
    currentBrightness = parseInt(e.target.value);
    localStorage.setItem('rgbBrightness', currentBrightness);
    await applyRgb();
});
sliderBrightness.addEventListener('input', (e) => valBrightness.innerText = `${e.target.value}%`);

const sliderSpeed = document.querySelector('.slider-speed');
const valSpeed = document.querySelector('.val-speed');
sliderSpeed.addEventListener('change', async (e) => {
    const val = parseInt(e.target.value);
    currentRgbSpeed = speedFromIndex(val);
    localStorage.setItem('rgbSpeedIndex', val);
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

// Initialize UI state on load
async function restoreRgbState() {
    const savedMode = localStorage.getItem('rgbMode');
    const savedBrightness = localStorage.getItem('rgbBrightness');
    const savedSpeedIndex = localStorage.getItem('rgbSpeedIndex');
    
    // Default fallback colors for zones if not saved (Hardware defaults)
    const defaultColors = {
        1: 'rgb(255, 0, 0)',    // Red
        2: 'rgb(0, 255, 0)',    // Green
        3: 'rgb(0, 0, 255)',    // Blue
        4: 'rgb(255, 255, 0)'   // Yellow
    };

    // Restore and apply zone colors to hardware & UI on startup
    for (let z = 1; z <= 4; z++) {
        const savedColor = localStorage.getItem(`rgbZoneColor_${z}`) || defaultColors[z];
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

    if (savedBrightness) {
        currentBrightness = parseInt(savedBrightness);
        sliderBrightness.value = currentBrightness;
        valBrightness.innerText = `${currentBrightness}%`;
    }
    
    if (savedSpeedIndex) {
        const val = parseInt(savedSpeedIndex);
        sliderSpeed.value = val;
        valSpeed.innerText = speedLabel(val);
        currentRgbSpeed = speedFromIndex(val);
    }
    
    if (savedMode) {
        // Trigger preset click logic
        const targetBtn = document.querySelector(`.preset-btn[data-preset="${savedMode}"]`);
        if (targetBtn) {
            targetBtn.click();
        }
    } else {
        const activePreset = document.querySelector('.preset-btn.active');
        if (activePreset && activePreset.getAttribute('data-preset') === 'static') {
            const initSpeedGroup = document.querySelector('.slider-speed').closest('.slider-group');
            initSpeedGroup.classList.add('disabled');
            document.querySelector('.slider-speed').disabled = true;
        }
    }

    // Apply overall settings (mode, speed, brightness)
    await applyRgb();

    // Select the default active zone (Zone 1) to update the palette selection UI
    const defaultZoneEl = document.querySelector(`.kb-zone-${activeZone}`);
    if (defaultZoneEl) {
        defaultZoneEl.click();
    }
}
restoreRgbState();
