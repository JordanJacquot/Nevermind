// Lanceur de Nexo: démarre fichiers\OptiGame.ps1 avec les droits administrateur.
// Compilé deux fois par construire.ps1 : Nexo.exe et Désinstaller Nexo.exe (symbole UNINSTALL).
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Windows.Forms;

[assembly: AssemblyTitle("Nexo")]
[assembly: AssemblyProduct("Nexo")]
[assembly: AssemblyDescription("Analyse et optimisation gaming pour Windows")]
[assembly: AssemblyVersion("1.0.0.0")]
[assembly: AssemblyFileVersion("1.0.0.0")]

static class Lanceur
{
    [STAThread]
    static int Main()
    {
        Application.EnableVisualStyles();
        string dossier = AppDomain.CurrentDomain.BaseDirectory;
        string script = Path.Combine(Path.Combine(dossier, "fichiers"), "OptiGame.ps1");

        if (!File.Exists(script))
        {
            MessageBox.Show(
                "Le dossier « fichiers » est introuvable à côté de ce programme.\n\n" +
                "Extrais d'abord tout le zip (clic droit sur le zip, puis « Extraire tout »), " +
                "puis lance Nexo depuis le dossier extrait.",
                "Nexo", MessageBoxButtons.OK, MessageBoxIcon.Warning);
            return 1;
        }

#if UNINSTALL
        string option = " -Uninstall";
#else
        string option = "";
#endif
        ProcessStartInfo psi = new ProcessStartInfo();
        psi.FileName = Path.Combine(Environment.SystemDirectory, @"WindowsPowerShell\v1.0\powershell.exe");
        psi.Arguments = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"" + script + "\"" + option;
        psi.UseShellExecute = true;
        psi.Verb = "runas";
        psi.WindowStyle = ProcessWindowStyle.Hidden;
        psi.WorkingDirectory = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);

        try
        {
            Process.Start(psi);
        }
        catch (Win32Exception)
        {
            MessageBox.Show(
                "Nexo a besoin des droits administrateur pour analyser et régler Windows.\n\n" +
                "Relance-le et clique sur « Oui » quand Windows le demande.",
                "Nexo", MessageBoxButtons.OK, MessageBoxIcon.Information);
            return 1;
        }
        return 0;
    }
}
