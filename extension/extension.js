/* Antigravity KeepAwake GNOME Shell 42 Top Bar Extension
 *
 * Sits in the top bar right box (between Flameshot & Cloudflare WARP)
 * and communicates with the Rust background daemon via Session D-Bus (`org.antigravity.KeepAwake`).
 */

const { GObject, St, Clutter, Gio, GLib } = imports.gi;
const Main = imports.ui.main;
const PanelMenu = imports.ui.panelMenu;
const PopupMenu = imports.ui.popupMenu;
const ExtensionUtils = imports.misc.extensionUtils;
const Me = ExtensionUtils.getCurrentExtension();

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

function getCoffeeIcon(filename) {
    let path = GLib.build_filenamev([Me.path, 'icons', filename]);
    let file = Gio.File.new_for_path(path);
    return new Gio.FileIcon({ file });
}

var KeepAwakeIndicator = GObject.registerClass(
class KeepAwakeIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.5, 'Antigravity KeepAwake', false);

        // Panel Container
        this._box = new St.BoxLayout({
            style_class: 'keepawake-panel-box',
            reactive: true,
            can_focus: true,
            track_hover: true,
        });

        // Coffee Cup Vector Icon (defaults to clean idle cup on launch)
        this._icon = new St.Icon({
            gicon: getCoffeeIcon('coffee-idle.svg'),
            style_class: 'system-status-icon',
            icon_size: 16,
            y_align: Clutter.ActorAlign.CENTER,
        });

        this._box.add_child(this._icon);
        this.add_child(this._box);

        // Connect D-Bus Proxy
        this._proxy = null;
        this._connecting = false;
        this._pollTimer = null;
        this._initDBus();

        // Build Popover Menu
        this._buildMenu();

        // Periodic telemetry refresh every 2.5s
        this._pollTimer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 2, () => {
            this._refreshStatus();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _initDBus() {
        if (this._connecting) {
            return;
        }
        this._connecting = true;

        try {
            new KeepAwakeProxy(
                Gio.DBus.session,
                DBUS_NAME,
                DBUS_PATH,
                (proxy, error) => {
                    this._connecting = false;
                    if (error) {
                        this._proxy = null;
                        this._updateUIOffline();
                    } else {
                        this._proxy = proxy;
                        this._refreshStatus();
                    }
                }
            );
        } catch (e) {
            this._connecting = false;
            this._proxy = null;
            this._updateUIOffline();
        }
    }

    _buildMenu() {
        // 1. Header Title
        let titleItem = new PopupMenu.PopupMenuItem('Antigravity KeepAwake', {
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
            } else {
                this._initDBus();
            }
        });
        this.menu.addMenuItem(this._masterSwitch);

        // 3. Operating Mode Section
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        let modeHeader = new PopupMenu.PopupMenuItem('Operating Mode:', { reactive: false });
        modeHeader.label.style = 'font-weight: bold; font-size: 11px; color: #888;';
        this.menu.addMenuItem(modeHeader);

        this._modeAutoItem = new PopupMenu.PopupMenuItem('Only Antigravity Always Awake');
        this._modeAutoItem.setOrnament(PopupMenu.Ornament.DOT);
        this._modeAutoItem.connect('activate', () => {
            if (this._proxy) {
                this._proxy.SetModeRemote(0, () => this._refreshStatus());
            } else {
                this._initDBus();
            }
        });
        this.menu.addMenuItem(this._modeAutoItem);

        this._modeAlwaysItem = new PopupMenu.PopupMenuItem('All Apps Awake');
        this._modeAlwaysItem.setOrnament(PopupMenu.Ornament.NONE);
        this._modeAlwaysItem.connect('activate', () => {
            if (this._proxy) {
                this._proxy.SetModeRemote(1, () => this._refreshStatus());
            } else {
                this._initDBus();
            }
        });
        this.menu.addMenuItem(this._modeAlwaysItem);

        // 4. Live System Telemetry
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        let telemetryHeader = new PopupMenu.PopupMenuItem('SYSTEM TELEMETRY', { reactive: false });
        telemetryHeader.label.style_class = 'keepawake-telemetry-header';
        this.menu.addMenuItem(telemetryHeader);

        this._statusRow = new PopupMenu.PopupMenuItem('Agent Status: Checking...', { reactive: false });
        this._statusRow.label.style_class = 'keepawake-telemetry-row';
        this.menu.addMenuItem(this._statusRow);

        this._lidRow = new PopupMenu.PopupMenuItem('Laptop Lid: Open', { reactive: false });
        this._lidRow.label.style_class = 'keepawake-telemetry-row';
        this.menu.addMenuItem(this._lidRow);

        this._frozenRow = new PopupMenu.PopupMenuItem('Frozen Apps: 0 processes', { reactive: false });
        this._frozenRow.label.style_class = 'keepawake-telemetry-row';
        this.menu.addMenuItem(this._frozenRow);

        this._hardwareRow = new PopupMenu.PopupMenuItem('Hardware: --°C | --%', { reactive: false });
        this._hardwareRow.label.style_class = 'keepawake-telemetry-row';
        this.menu.addMenuItem(this._hardwareRow);
    }

    _refreshStatus() {
        if (!this._proxy) {
            this._initDBus();
            return;
        }

        this._proxy.GetStatusRemote((result, error) => {
            if (error || !result) {
                this._proxy = null;
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

                // 1. Update Top Bar Coffee Cup Icon
                if (!enabled) {
                    // Disabled by user -> Slashed coffee cup
                    this._icon.gicon = getCoffeeIcon('coffee-disabled.svg');
                } else if (inhibited || actState === 'Active' || mode === 1) {
                    // Active guarding -> Steaming warm golden coffee cup
                    this._icon.gicon = getCoffeeIcon('coffee-active.svg');
                } else {
                    // Enabled, waiting/idle -> Clean calm coffee cup (NO slash)
                    this._icon.gicon = getCoffeeIcon('coffee-idle.svg');
                }

                // 2. Update Switches & Radio Options
                this._masterSwitch.setToggleState(enabled);

                if (mode === 0) {
                    this._modeAutoItem.setOrnament(PopupMenu.Ornament.DOT);
                    this._modeAlwaysItem.setOrnament(PopupMenu.Ornament.NONE);
                } else {
                    this._modeAutoItem.setOrnament(PopupMenu.Ornament.NONE);
                    this._modeAlwaysItem.setOrnament(PopupMenu.Ornament.DOT);
                }

                // 3. Update Telemetry Rows
                let actText = actState;
                if (actState === 'GracePeriod') {
                    actText = `Grace Period (${remaining}s)`;
                }
                this._statusRow.label.text = `Agent Status: ${actText}`;
                this._lidRow.label.text = `Laptop Lid: ${lidClosed ? 'Closed (Inhibited)' : 'Open'}`;
                this._frozenRow.label.text = `Frozen Apps: ${frozenCount} processes`;
                this._hardwareRow.label.text = `Hardware: ${tempC.toFixed(1)}°C | ${batteryPct}%`;
            } catch (e) {
                log(`[KeepAwake] Error parsing status dict: ${e.message}`);
            }
        });
    }

    _updateUIOffline() {
        this._icon.gicon = getCoffeeIcon('coffee-disabled.svg');
        this._statusRow.label.text = 'Agent Status: Daemon Offline';
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
    // Place in right box among the tray indicators (between Flameshot & WARP)
    Main.panel.addToStatusArea('antigravity-keepawake', indicator, 1, 'right');
}

function disable() {
    if (indicator) {
        indicator.destroy();
        indicator = null;
    }
}
