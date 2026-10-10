// Lanceur de Nevermind : exécute fichiers\OptiGame.ps1 DANS son propre processus (moteur PowerShell hébergé),
// pour que le Gestionnaire des tâches et la barre des tâches affichent « Nevermind » avec son icône, pas « Windows PowerShell ».
// Les droits administrateur sont demandés par le manifeste (une seule fenêtre de confirmation de Windows).
// Compilé deux fois par construire.ps1 : Nevermind.exe et Désinstaller Nevermind.exe (symbole UNINSTALL).
using System;
using System.IO;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;
using System.Threading;
using System.Windows.Forms;

[assembly: AssemblyTitle("Nevermind")]
[assembly: AssemblyProduct("Nevermind")]
[assembly: AssemblyDescription("Nevermind")]
[assembly: AssemblyCompany("Nevermind")]
[assembly: AssemblyVersion("1.0.0.0")]
[assembly: AssemblyFileVersion("1.0.0.0")]

static class Lanceur
{
    [STAThread]
    static int Main(string[] args)
    {
        Application.EnableVisualStyles();
        string dossier = AppDomain.CurrentDomain.BaseDirectory;
        string script = Path.Combine(Path.Combine(dossier, "fichiers"), "OptiGame.ps1");

        if (!File.Exists(script))
        {
            MessageBox.Show(
                "Le dossier « fichiers » est introuvable à côté de ce programme.\n\n" +
                "Extrais d'abord tout le zip (clic droit sur le zip, puis « Extraire tout »), " +
                "puis lance Nevermind depuis le dossier extrait.",
                "Nevermind", MessageBoxButtons.OK, MessageBoxIcon.Warning);
            return 1;
        }

#if UNINSTALL
        bool uninstall = true;
#else
        bool uninstall = false;
#endif
        bool demarrage = false;
        foreach (string a in args) { if (a.Trim('-', '/').Equals("Demarrage", StringComparison.OrdinalIgnoreCase)) demarrage = true; }

        try
        {
            Directory.SetCurrentDirectory(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile));
            InitialSessionState iss = InitialSessionState.CreateDefault();
            iss.ExecutionPolicy = Microsoft.PowerShell.ExecutionPolicy.Bypass;
            using (Runspace rs = RunspaceFactory.CreateRunspace(iss))
            {
                // Le fil principal (STA) fait tourner la fenêtre WPF de l'app
                rs.ApartmentState = ApartmentState.STA;
                rs.ThreadOptions = PSThreadOptions.UseCurrentThread;
                rs.Open();
                using (PowerShell ps = PowerShell.Create())
                {
                    ps.Runspace = rs;
                    ps.AddCommand(script);
                    if (uninstall) ps.AddParameter("Uninstall");
                    if (demarrage) ps.AddParameter("Demarrage");
                    ps.Invoke();
                }
            }
        }
        catch (Exception e)
        {
            string detail = e.Message;
            RuntimeException re = e as RuntimeException;
            if (re != null && re.ErrorRecord != null && re.ErrorRecord.InvocationInfo != null)
                detail += "\n\n(" + Path.GetFileName(re.ErrorRecord.InvocationInfo.ScriptName) + ", ligne " + re.ErrorRecord.InvocationInfo.ScriptLineNumber + ")";
            try
            {
                string log = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "OptiGame");
                Directory.CreateDirectory(log);
                File.AppendAllText(Path.Combine(log, "journal.txt"), DateTime.Now.ToString("s") + " ERREUR au lancement : " + detail.Replace("\n", " ") + "\r\n");
            }
            catch { }
            MessageBox.Show("Nevermind n'a pas pu démarrer :\n\n" + detail, "Nevermind", MessageBoxButtons.OK, MessageBoxIcon.Warning);
            return 1;
        }
        return 0;
    }
}
