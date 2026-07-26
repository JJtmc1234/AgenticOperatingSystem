using Aos.Hud.Native;

namespace Aos.Hud;

/// <summary>
/// The overlay. Hidden until the hotkey summons it, holds one persistent agent session, and
/// lives in the tray the rest of the time.
/// </summary>
internal sealed class HudForm : Form
{
    private const int HotKeyId = 0xA05;
    private const string HotKeyLabel = "Ctrl+Alt+Space";

    private readonly AgentSession _session;
    private readonly NotifyIcon _tray;
    private readonly TextBox _output;
    private readonly TextBox _input;
    private readonly Label _status;

    private bool _shuttingDown;

    public HudForm(string agentScript, string workingDirectory)
    {
        _session = new AgentSession(agentScript, workingDirectory);
        _session.OutputReceived += line => BeginInvoke(() => Append(line));
        _session.Exited += code => BeginInvoke(() => OnAgentExited(code));

        Text = "AgenticOS";
        StartPosition = FormStartPosition.CenterScreen;
        Size = new Size(900, 560);
        MinimumSize = new Size(560, 320);
        // Starts hidden: the hotkey is the way in, and a window that appears at login is a
        // nuisance rather than a feature.
        ShowInTaskbar = false;
        FormBorderStyle = FormBorderStyle.Sizable;
        KeyPreview = true;

        _output = new TextBox
        {
            Multiline = true,
            ReadOnly = true,
            ScrollBars = ScrollBars.Vertical,
            Dock = DockStyle.Fill,
            Font = new Font("Consolas", 10f),
            BackColor = Color.FromArgb(24, 24, 27),
            ForeColor = Color.FromArgb(228, 228, 231),
            BorderStyle = BorderStyle.None,
        };

        _input = new TextBox
        {
            Dock = DockStyle.Fill,
            Font = new Font("Consolas", 11f),
            BorderStyle = BorderStyle.FixedSingle,
        };
        _input.KeyDown += OnInputKeyDown;

        _status = new Label
        {
            Dock = DockStyle.Bottom,
            Height = 22,
            TextAlign = ContentAlignment.MiddleLeft,
            ForeColor = Color.FromArgb(120, 120, 130),
            Text = "starting...",
        };

        var inputPanel = new Panel { Dock = DockStyle.Bottom, Height = 32, Padding = new Padding(0, 4, 0, 0) };
        inputPanel.Controls.Add(_input);

        Controls.Add(_output);
        Controls.Add(inputPanel);
        Controls.Add(_status);

        _tray = new NotifyIcon
        {
            Icon = SystemIcons.Application,
            Text = $"AgenticOS ({HotKeyLabel})",
            Visible = true,
        };
        _tray.DoubleClick += (_, _) => Summon();
        _tray.ContextMenuStrip = BuildTrayMenu();
    }

    private ContextMenuStrip BuildTrayMenu()
    {
        var menu = new ContextMenuStrip();
        menu.Items.Add($"Show ({HotKeyLabel})", null, (_, _) => Summon());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Restart agent", null, (_, _) => RestartAgent());
        // The kill switch. Stopping the session revokes every capability at once, because
        // the broker only exists inside that process.
        menu.Items.Add("Halt agent (kill switch)", null, (_, _) =>
        {
            _session.Stop();
            Append("[halted by user; use Restart agent to bring it back]");
            SetStatus("halted");
        });
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Exit", null, (_, _) => ExitApplication());
        return menu;
    }

    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);

        if (!HotKey.TryRegister(Handle, HotKeyId, HotKey.Modifiers.Control | HotKey.Modifiers.Alt, Keys.Space))
        {
            // Another process owns the combination. Windows will not say which, so the only
            // honest response is to keep running and tell the user the tray still works.
            _tray.ShowBalloonTip(
                5000,
                "AgenticOS",
                $"{HotKeyLabel} is already taken by another app. Use the tray icon instead.",
                ToolTipIcon.Warning);
        }

        StartAgent();
    }

    protected override void WndProc(ref Message m)
    {
        if (m.Msg == HotKey.WmHotKey && m.WParam.ToInt32() == HotKeyId)
        {
            // Pressing the hotkey while visible dismisses it, so the same key toggles.
            if (Visible && !WindowState.Equals(FormWindowState.Minimized)) { Hide(); }
            else { Summon(); }
            return;
        }

        base.WndProc(ref m);
    }

    private void StartAgent()
    {
        try
        {
            SetStatus("starting agent, this takes a few seconds on first launch...");
            _session.Start();
            SetStatus($"ready. {HotKeyLabel} shows and hides this window.");
        }
        catch (Exception ex)
        {
            Append($"[could not start the agent: {ex.Message}]");
            SetStatus("agent not running");
        }
    }

    private void RestartAgent()
    {
        _session.Stop();
        _output.Clear();
        StartAgent();
    }

    private void OnAgentExited(int code)
    {
        if (_shuttingDown) { return; }
        Append($"[agent exited with code {code}]");
        SetStatus("agent stopped. Tray menu has Restart agent.");
    }

    private void Summon()
    {
        Show();
        if (WindowState == FormWindowState.Minimized) { WindowState = FormWindowState.Normal; }
        HotKey.ShowWindow(Handle, HotKey.SwRestore);
        // Activate() alone loses to whatever currently owns the foreground; the Win32 call
        // is what actually raises the window when summoned from another app.
        HotKey.SetForegroundWindow(Handle);
        _input.Focus();
    }

    private void OnInputKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.KeyCode == Keys.Escape) { Hide(); e.Handled = true; return; }

        // Shift+Enter inserts a newline instead of sending, matching every chat box.
        if (e.KeyCode != Keys.Enter || e.Shift) { return; }

        e.Handled = true;
        e.SuppressKeyPress = true;

        var request = _input.Text.Trim();
        if (request.Length == 0) { return; }

        if (!_session.IsRunning)
        {
            Append("[the agent is not running; use Restart agent in the tray menu]");
            return;
        }

        Append($"> {request}");
        _input.Clear();

        try
        {
            _session.Send(request);
        }
        catch (Exception ex)
        {
            Append($"[could not send: {ex.Message}]");
        }
    }

    private void Append(string line)
    {
        // The agent echoes its own "> " prompt for a terminal; the window already shows the
        // request, so the bare prompt is noise here.
        var text = line.TrimStart();
        if (text is ">" or "> ") { return; }

        _output.AppendText(line + Environment.NewLine);
        _output.SelectionStart = _output.TextLength;
        _output.ScrollToCaret();
    }

    private void SetStatus(string text) => _status.Text = text;

    protected override void OnFormClosing(FormClosingEventArgs e)
    {
        // Closing the window hides it rather than exiting, which is what a tray app should
        // do. Exit is deliberate, from the tray menu.
        if (!_shuttingDown && e.CloseReason == CloseReason.UserClosing)
        {
            e.Cancel = true;
            Hide();
            return;
        }

        base.OnFormClosing(e);
    }

    private void ExitApplication()
    {
        _shuttingDown = true;
        Close();
        Application.Exit();
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            HotKey.Unregister(Handle, HotKeyId);
            _session.Dispose();
            _tray.Visible = false;
            _tray.Dispose();
        }

        base.Dispose(disposing);
    }
}
