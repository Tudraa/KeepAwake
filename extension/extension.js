/* Antigravity KeepAwake GNOME Shell 42 Top Bar Extension
 *
 * Sits in the top bar beside the clock and communicates with the
 * Rust background daemon via Session D-Bus (`org.antigravity.KeepAwake`).
 */

const { GObject, St, Clutter, Gio, GLib } = imports.gi;
const Main = imports.ui.main;
const PanelMenu = imports.ui.panelMenu;
const PopupMenu = imports.ui.popupMenu;

const DBUS_NAME = 'org.antigravity.KeepAwake';
const DBUS_PATH = '/org/antigravity/KeepAwake';
const DBUS_IFACE = 'org.antigravity.KeepAwake';

const KeepAwakeIface = `
<node>
  <interface name="org.antigravity.KeepAwake">
    <method name="Toggle">
      <arg type="b" name="enabled" direction="out" />
    </method>
    <method name="SetMode">
      <arg type="u" name="mode" direction="in" />
    </method>
    <method name="GetStatus">
      <arg type="a{sv}" name="status" direction="out" />
    </method>
    <method name="Ping">
      <arg type="s" name="reply" direction="out" />
    </method>
  </interface>
</node>
`;

const KeepAwakeProxy = Gio.DBusProxy.makeProxyWrapper(KeepAwakeIface);

var KeepAwakeIndicator = GObject.registerClass(
class KeepAwakeIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.5, 'Antigravity KeepAwake', false);

        // Panel Label / Icon container
        this._box = new St.BoxLayout({
            style_class: 'keepawake-panel-box',
            reactive: true,
            can_focus: true,
            track_hover: true,
        });

        this._iconLabel = new St.Label({
            text: '⚡',
            y_align: Clutter.ActorAlign.CENTER,
            style_class: 'keepawake-icon keepawake-icon-active',
        });

        this._box.add_child(this._iconLabel);
        this.add_child(this._box);

        // Connect D-Bus Proxy
        this._proxy = null;
        this._pollTimer = null;
        this._initDBus();

        // Build Popover Menu
        this._buildMenu();
    }

    _initDBus() {
        try {
            this._proxy = new KeepAwakeProxy(
                Gio.DBus.session,
                DBUS_NAME,
                DBUS_PATH,
                (proxy, error) => {
                    if (error) {
                        log(`[KeepAwake] D-Bus connection failed: ${error.message}`);
                        this._updateUIOffline();
                    } else {
                        this._proxy = proxy;
                        this._refreshStatus();
                    }
                }
            );
        } catch (e) {
            log(`[KeepAwake] Exception creating proxy: ${e.message}`);
            this._updateUIOffline();
        }

        // Periodic telemetry refresh every 2.5s
        this._pollTimer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 3, () => {
            this._refreshStatus();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _buildMenu() {
        // 1. Header Title
        let titleItem = new PopupMenu.PopupMenuItem('🚀 Antigravity KeepAwake', {
            reactive: false,
        });
        titleItem.label.style = 'font-weight: bold; font-size: 13px;';
        this.menu.addMenuItem(titleItem);
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        // 2. Master Toggle Switch
        this._masterSwitch = new PopupMenu.PopupSwitchMenuItem('Master Enable', true);
        this._masterSwitch.connect('toggled', (item, state) => {
            if (this._proxy) {
                this._proxy.ToggleRemote((result, error) => {
                    if (!error) {
                        this._refreshStatus();
                    }
                });
            }
        });
        this.menu.addMenuItem(this._masterSwitch);

        // 3. Mode Radio Section
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        let modeHeader = new PopupMenu.PopupMenuItem('Pilihan Mode:', { reactive: false });
        modeHeader.label.style = 'font-weight: bold; font-size: 11px; color: #888;';
        this.menu.addMenuItem(modeHeader);

        this._modeAutoItem = new PopupMenu.PopupMenuItem('  ⦿ Mode Otomatis (Ikut Antigravity)');
        this._modeAutoItem.connect('activate', () => {
            if (this._proxy) {
                this._proxy.SetModeRemote(0, () => this._refreshStatus());
            }
        });
        this.menu.addMenuItem(this._modeAutoItem);

        this._modeAlwaysItem = new PopupMenu.PopupMenuItem('  ○ Selalu Terjaga (Always Awake)');
        this._modeAlwaysItem.connect('activate', () => {
            if (this._proxy) {
                this._proxy.SetModeRemote(1, () => this._refreshStatus());
            }
        });
        this.menu.addMenuItem(this._modeAlwaysItem);

        // 4. Live Telemetry
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        let telemetryHeader = new PopupMenu.PopupMenuItem('TELEMETRI SISTEM', { reactive: false });
        telemetryHeader.label.style_class = 'keepawake-telemetry-header';
        this.menu.addMenuItem(telemetryHeader);

        this._statusRow = new PopupMenu.PopupMenuItem('Status Agen: Memeriksa...', { reactive: false });
        this._statusRow.label.style_class = 'keepawake-telemetry-row';
        this.menu.addMenuItem(this._statusRow);

        this._lidRow = new PopupMenu.PopupMenuItem('Engsel Layar: Terbuka', { reactive: false });
        this._lidRow.label.style_class = 'keepawake-telemetry-row';
        this.menu.addMenuItem(this._lidRow);

        this._frozenRow = new PopupMenu.PopupMenuItem('Aplikasi Dibekukan: 0 proses', { reactive: false });
        this._frozenRow.label.style_class = 'keepawake-telemetry-row';
        this.menu.addMenuItem(this._frozenRow);

        this._hardwareRow = new PopupMenu.PopupMenuItem('Suhu & Baterai: --°C | --%', { reactive: false });
        this._hardwareRow.label.style_class = 'keepawake-telemetry-row';
        this.menu.addMenuItem(this._hardwareRow);
    }

    _refreshStatus() {
        if (!this._proxy) {
            this._updateUIOffline();
            return;
        }

        this._proxy.GetStatusRemote((result, error) => {
            if (error || !result) {
                this._updateUIOffline();
                return;
            }

            try {
                let statusDict = result[0];
                let enabled = statusDict['enabled'] ? statusDict['enabled'].deepUnpack() : false;
                let mode = statusDict['mode'] ? statusDict['mode'].deepUnpack() : 0;
                let actState = statusDict['activity_state'] ? statusDict['activity_state'].deepUnpack() : 'Idle';
                let remaining = statusDict['grace_remaining_secs'] ? statusDict['grace_remaining_secs'].deepUnpack() : 0;
                let lidClosed = statusDict['lid_closed'] ? statusDict['lid_closed'].deepUnpack() : false;
                let inhibited = statusDict['inhibited'] ? statusDict['inhibited'].deepUnpack() : false;
                let frozenCount = statusDict['frozen_count'] ? statusDict['frozen_count'].deepUnpack() : 0;
                let tempC = statusDict['cpu_temp_c'] ? statusDict['cpu_temp_c'].deepUnpack() : 0.0;
                let batteryPct = statusDict['battery_pct'] ? statusDict['battery_pct'].deepUnpack() : 100;

                // 1. Update Top Bar Icon
                if (!enabled) {
                    this._iconLabel.text = '🛑';
                    this._iconLabel.style_class = 'keepawake-icon keepawake-icon-disabled';
                } else if (inhibited) {
                    this._iconLabel.text = '⚡';
                    this._iconLabel.style_class = 'keepawake-icon keepawake-icon-active';
                } else {
                    this._iconLabel.text = '💤';
                    this._iconLabel.style_class = 'keepawake-icon keepawake-icon-idle';
                }

                // 2. Update Switches & Radio
                this._masterSwitch.setToggleState(enabled);

                if (mode === 0) {
                    this._modeAutoItem.label.text = '  ⦿ Mode Otomatis (Ikut Antigravity)';
                    this._modeAlwaysItem.label.text = '  ○ Selalu Terjaga (Always Awake)';
                } else {
                    this._modeAutoItem.label.text = '  ○ Mode Otomatis (Ikut Antigravity)';
                    this._modeAlwaysItem.label.text = '  ⦿ Selalu Terjaga (Always Awake)';
                }

                // 3. Update Telemetry
                let actText = actState;
                if (actState === 'GracePeriod') {
                    actText = `Masa Tenggang (${remaining}s)`;
                }
                this._statusRow.label.text = `Status Agen: ${actText}`;
                this._lidRow.label.text = `Engsel Layar: ${lidClosed ? 'Tertutup (Locked)' : 'Terbuka'}`;
                this._frozenRow.label.text = `Aplikasi Dibekukan: ${frozenCount} proses`;
                this._hardwareRow.label.text = `Suhu & Baterai: ${tempC.toFixed(1)}°C | ${batteryPct}%`;
            } catch (e) {
                log(`[KeepAwake] Error parsing status dict: ${e.message}`);
            }
        });
    }

    _updateUIOffline() {
        this._iconLabel.text = '🛑';
        this._iconLabel.style_class = 'keepawake-icon keepawake-icon-disabled';
        this._statusRow.label.text = 'Status Agen: Daemon Offline';
    }

    destroy() {
        if (this._pollTimer) {
            GLib.source_remove(this._pollTimer);
            this._pollTimer = null;
        }
        super.destroy();
    }
});

var indicator = null;

function init() {
    // Extension initialization
}

function enable() {
    indicator = new KeepAwakeIndicator();
    // Place right beside clock in centerBox
    Main.panel.addToStatusArea('antigravity-keepawake', indicator, 1, 'center');
}

function disable() {
    if (indicator) {
        indicator.destroy();
        indicator = null;
    }
}
