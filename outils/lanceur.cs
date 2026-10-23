// Lanceur de Nevermind : exécute fichiers\OptiGame.ps1 DANS son propre processus (moteur PowerShell hébergé),
// pour que le Gestionnaire des tâches et la barre des tâches affichent « Nevermind » avec son icône, pas « Windows PowerShell ».
// Il démarre sans droits (manifeste « asInvoker ») : OptiGame.ps1 se relance lui-même en administrateur après ses
// premiers contrôles (copie gérée par Mindset, Nevermind déjà ouvert), une seule fenêtre de confirmation de Windows.
// Compilé trois fois par construire.ps1 : Nevermind.exe, Désinstaller Nevermind.exe (symbole UNINSTALL) et
// NevermindAide.exe (symbole AIDE : la partie administrateur, fichiers\NevermindAide.ps1).
using System;
using System.Collections.Generic;
using System.IO;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;
using System.Threading;
using System.Web.Script.Serialization;
using System.Windows.Forms;

[assembly: AssemblyTitle("Nevermind")]
[assembly: AssemblyProduct("Nevermind")]
[assembly: AssemblyDescription("Nevermind")]
[assembly: AssemblyCompany("Nevermind")]
[assembly: AssemblyVersion("1.0.0.0")]
[assembly: AssemblyFileVersion("1.0.0.0")]

static class Lanceur
{
    // Dossier des données, même règle que Resolve-DataDir (OptiGame.ps1) : si %LOCALAPPDATA%\Mindset\suite.json note
    // cette copie comme celle de Mindset (et que l'ancien dossier a été renommé), <X>:\Mindset\Donnees\Nevermind pour un
    // programme dans <X>:\Mindset\Programmes\Nevermind, sinon %LOCALAPPDATA%\Mindset\Nevermind ; autrement
    // %LOCALAPPDATA%\OptiGame. Le dossier lu dans suite.json ne sert qu'à comparer.
    static string DossierGere(string racine, string local)
    {
        string p = Path.GetDirectoryName(Path.GetFullPath(racine).TrimEnd('\\'));
        string m = p != null ? Path.GetDirectoryName(p) : null;
        if (m != null && string.Equals(Path.GetFileName(p), "Programmes", StringComparison.OrdinalIgnoreCase) &&
            string.Equals(Path.GetFileName(m), "Mindset", StringComparison.OrdinalIgnoreCase))
            return Path.Combine(Path.Combine(m, "Donnees"), "Nevermind");
        return Path.Combine(Path.Combine(local, "Mindset"), "Nevermind");
    }

    static string DossierDonnees(string racine)
    {
        string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        string ancien = Path.Combine(local, "OptiGame");
        try
        {
            string nouveau = DossierGere(racine, local);
            var suite = new JavaScriptSerializer().DeserializeObject(File.ReadAllText(Path.Combine(Path.Combine(local, "Mindset"), "suite.json"))) as Dictionary<string, object>;
            object applis, dossier;
            if (suite != null && suite.TryGetValue("applis", out applis) && applis is Dictionary<string, object> &&
                ((Dictionary<string, object>)applis).TryGetValue("nevermind", out dossier) && dossier is string && Path.IsPathRooted((string)dossier) &&
                string.Equals(Path.GetFullPath((string)dossier).TrimEnd('\\'), Path.GetFullPath(racine).TrimEnd('\\'), StringComparison.OrdinalIgnoreCase) &&
                (Directory.Exists(nouveau) || !Directory.Exists(ancien)))
                return nouveau;
        }
        catch { }
        return ancien;
    }

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

#if AIDE
        // NevermindAide.exe : la partie administrateur de Nevermind (fichiers\NevermindAide.ps1), lancée par la tâche
        // « \Mindset\Nevermind (aide) ». Programme à part : Mindset et le Gestionnaire des tâches ne la confondent pas
        // avec Nevermind. Sans fenêtre ; en cas de problème, rien n'est écrit (dossier modifiable sans droits).
        {
            try
            {
                Directory.SetCurrentDirectory(Environment.GetFolderPath(Environment.SpecialFolder.System));
                InitialSessionState issAide = InitialSessionState.CreateDefault();
                issAide.ExecutionPolicy = Microsoft.PowerShell.ExecutionPolicy.Bypass;
                using (Runspace rsAide = RunspaceFactory.CreateRunspace(issAide))
                {
                    rsAide.Open();
                    using (PowerShell psAide = PowerShell.Create())
                    {
                        psAide.Runspace = rsAide;
                        psAide.AddCommand(Path.Combine(Path.Combine(dossier, "fichiers"), "NevermindAide.ps1"));
                        psAide.Invoke();
                    }
                }
            }
            catch { Environment.Exit(1); }
            Environment.Exit(0);   // aucun fil laissé par l'Aide ne garde le processus ouvert
            return 0;
        }
#else
#if UNINSTALL
        bool uninstall = true;
#else
        bool uninstall = false;
#endif
        bool demarrage = false;
        string donnees = null;
        foreach (string a in args)
        {
            string nom = a.Trim('-', '/');
            if (nom.Equals("Demarrage", StringComparison.OrdinalIgnoreCase)) demarrage = true;
            else if (nom.Equals("GarderDonnees", StringComparison.OrdinalIgnoreCase)) donnees = donnees == null || donnees == "garder" ? "garder" : "";
            else if (nom.Equals("SupprimerDonnees", StringComparison.OrdinalIgnoreCase)) donnees = donnees == null || donnees == "supprimer" ? "supprimer" : "";
        }
        // Désinstaller Nevermind.exe -GarderDonnees / -SupprimerDonnees (lancé par le désinstalleur de Mindset) : le choix
        // des données, sans question. Transmis au script par NEVERMIND_DONNEES, dans ce processus seulement ; une valeur
        // héritée d'ailleurs est effacée (seule la ligne de commande compte). Les deux à la fois : la question est posée.
        if (uninstall) Environment.SetEnvironmentVariable("NEVERMIND_DONNEES", string.IsNullOrEmpty(donnees) ? null : donnees);

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
            // Comme pour l'Aide : aucun fil de fond ne garde le processus ouvert une fois l'app fermée. Les groupes de fils
            // d'OptiGame.ps1 (RunspacePool) restés ouverts gardaient Nevermind.exe en vie sans fenêtre, avec son verrou :
            // Mindset ne pouvait plus le rouvrir. OptiGame.ps1 les ferme ; ceci reste en filet de sécurité.
            // (La relance après une mise à jour est déjà faite par le script, avant ce point.)
            Environment.Exit(0);
        }
        catch (Exception e)
        {
            string detail = e.Message;
            RuntimeException re = e as RuntimeException;
            if (re != null && re.ErrorRecord != null && re.ErrorRecord.InvocationInfo != null)
                detail += "\n\n(" + Path.GetFileName(re.ErrorRecord.InvocationInfo.ScriptName) + ", ligne " + re.ErrorRecord.InvocationInfo.ScriptLineNumber + ")";
            try
            {
                string log = DossierDonnees(dossier);
                Directory.CreateDirectory(log);
                File.AppendAllText(Path.Combine(log, "journal.txt"), DateTime.Now.ToString("s") + " ERREUR au lancement : " + detail.Replace("\n", " ") + "\r\n");
            }
            catch { }
            MessageBox.Show("Nevermind n'a pas pu démarrer :\n\n" + detail, "Nevermind", MessageBoxButtons.OK, MessageBoxIcon.Warning);
            Environment.Exit(1);   // (même raison : des fils de fond déjà lancés garderaient le processus ouvert)
        }
        return 0;
#endif
    }
}
