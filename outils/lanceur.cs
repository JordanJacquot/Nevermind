// Lanceur de Nevermind : exécute fichiers\Nevermind.ps1 DANS son propre processus (moteur PowerShell hébergé),
// pour que le Gestionnaire des tâches et la barre des tâches affichent son nom et son icône, pas « Windows PowerShell ».
// Il démarre sans droits (manifeste « asInvoker ») : Nevermind.ps1 se relance lui-même en administrateur après ses
// premiers contrôles (copie gérée par Mindset, Nevermind déjà ouvert), une seule fenêtre de confirmation de Windows.
// Compilé quatre fois par construire.ps1 :
//   Mindset.exe (sans symbole) : le programme qui fait tourner Nevermind. Toutes les applis de la suite tournent dans un
//     « Mindset.exe » de leur dossier : le Gestionnaire des tâches les range sur une seule ligne « Mindset ». Nevermind
//     garde son identité dans la barre des tâches (AppUserModelID JordanJacquot.Nevermind, evenements.ps1) et son verrou ;
//   Nevermind.exe (symbole RELAIS) : petit relais qui lance Mindset.exe de son dossier avec les mêmes arguments, puis se
//     ferme aussitôt. Les anciennes versions de Mindset, les anciens raccourcis et les anciennes tâches l'appellent encore ;
//   Désinstaller Nevermind.exe (symbole UNINSTALL) et NevermindAide.exe (symbole AIDE : la partie administrateur,
//     fichiers\NevermindAide.ps1).
// La version (celle de Nevermind.ps1) est ajoutée par construire.ps1 dans un petit fichier à part.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Management.Automation;
using System.Management.Automation.Runspaces;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Web.Script.Serialization;
using System.Windows.Forms;

// Le titre donne le libellé de la ligne dans le Gestionnaire des tâches : « Mindset » pour le programme qui tourne
#if UNINSTALL || AIDE || RELAIS
[assembly: AssemblyTitle("Nevermind")]
#else
[assembly: AssemblyTitle("Mindset")]
#endif
[assembly: AssemblyProduct("Nevermind")]
[assembly: AssemblyDescription("Nevermind")]
[assembly: AssemblyCompany("Nevermind")]

static class Lanceur
{
    // Dossier des données, même règle que Resolve-DataDir (Nevermind.ps1) : si %LOCALAPPDATA%\Mindset\suite.json note
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
        // Le script de Nevermind : fichiers\Nevermind.ps1, ou à défaut fichiers\OptiGame.ps1 (son ancien nom, gardé dans le zip
        // de compatibilité que reçoivent les Mindset 2.12.0 et plus anciens)
        string script = Path.Combine(Path.Combine(dossier, "fichiers"), "Nevermind.ps1");
        string ancienScript = Path.Combine(Path.Combine(dossier, "fichiers"), "OptiGame.ps1");
        if (!File.Exists(script) && File.Exists(ancienScript)) script = ancienScript;

#if RELAIS
        // Nevermind.exe : lance Mindset.exe de CE dossier (jamais un autre), avec les mêmes arguments, le même dossier de
        // travail, les mêmes variables et les mêmes droits (une tâche « avec les droits les plus élevés » les transmet),
        // puis se ferme aussitôt.
        {
            string hote = Path.Combine(dossier, "Mindset.exe");
            try
            {
                var psi = new ProcessStartInfo(hote, Arguments(args));
                psi.UseShellExecute = false;
                psi.WorkingDirectory = Environment.CurrentDirectory;
                using (Process.Start(psi)) { }
                return 0;
            }
            catch (Exception e)
            {
                MessageBox.Show("Nevermind n'a pas pu démarrer :\n\n" + (File.Exists(hote) ? e.Message : "Mindset.exe est introuvable dans " + dossier) +
                    "\n\nOuvre Mindset et réinstalle Nevermind.", "Nevermind", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return 1;
            }
        }
#else
#if AIDE
        // NevermindAide.exe : la partie administrateur de Nevermind (fichiers\NevermindAide.ps1), lancée par la tâche
        // « \Mindset\Nevermind (aide) », en SYSTEM, avec -Sid <utilisateur> (inscrit par l'administrateur qui a créé la
        // tâche), ou par la demande de Windows. Programme à part : Mindset et le Gestionnaire des tâches ne la confondent
        // pas avec Nevermind. Aucune fenêtre, jamais (en SYSTEM, personne ne la verrait et elle bloquerait l'Aide) ; en
        // cas de problème, rien n'est écrit (dossier modifiable sans droits).
        {
            string scriptAide = Path.Combine(Path.Combine(dossier, "fichiers"), "NevermindAide.ps1");
            if (!File.Exists(scriptAide)) return 1;
            string sid = null;
            for (int i = 0; i + 1 < args.Length; i++)
                if (args[i].Trim('-', '/').Equals("Sid", StringComparison.OrdinalIgnoreCase)) sid = args[i + 1];
            try
            {
                Directory.SetCurrentDirectory(Environment.GetFolderPath(Environment.SpecialFolder.System));
                EnvironnementAide();
                InitialSessionState issAide = InitialSessionState.CreateDefault();
                issAide.ExecutionPolicy = Microsoft.PowerShell.ExecutionPolicy.Bypass;
                using (Runspace rsAide = RunspaceFactory.CreateRunspace(issAide))
                {
                    rsAide.Open();
                    using (PowerShell psAide = PowerShell.Create())
                    {
                        psAide.Runspace = rsAide;
                        psAide.AddCommand(scriptAide);
                        if (sid != null) psAide.AddParameter("Sid", sid);   // (vérifié par le script)
                        psAide.Invoke();
                    }
                }
            }
            catch { Environment.Exit(1); }
            Environment.Exit(0);   // aucun fil laissé par l'Aide ne garde le processus ouvert
            return 0;
        }
#else
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
            // de Nevermind.ps1 (RunspacePool) restés ouverts gardaient le programme en vie sans fenêtre, avec son verrou :
            // Mindset ne pouvait plus le rouvrir. Nevermind.ps1 les ferme ; ceci reste en filet de sécurité.
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
#endif
    }

#if AIDE
    // L'Aide tourne en administrateur. En SYSTEM, elle ne reçoit rien de l'utilisateur ; mais lancée par la demande de
    // Windows (ou par une ancienne tâche « droits les plus élevés »), elle reçoit ses variables d'environnement, qu'un
    // programme sans droits peut changer (HKCU\Environment). Avant d'ouvrir le moteur PowerShell, les dossiers de
    // Windows sont remis tels que Windows les donne (pas lus dans ces variables), et les modules de PowerShell ne sont
    // cherchés que dans le dossier de Windows : un faux module posé dans Documents ou ailleurs n'est jamais chargé.
    // (LocalApplicationData et Documents ne sont pas repris ici : même lus par GetFolderPath, l'utilisateur peut les
    // déplacer. L'Aide ne s'en sert pas.)
    static void EnvironnementAide()
    {
        string windows = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
        string systeme = Environment.GetFolderPath(Environment.SpecialFolder.System);
        string programmes = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
        string programmesX86 = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86);
        string communes = Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData);
        if (string.IsNullOrEmpty(windows) || string.IsNullOrEmpty(systeme) || string.IsNullOrEmpty(programmes) || string.IsNullOrEmpty(communes))
            throw new InvalidOperationException("dossiers de Windows introuvables");
        Environment.SetEnvironmentVariable("windir", windows);
        Environment.SetEnvironmentVariable("SystemRoot", windows);
        Environment.SetEnvironmentVariable("ProgramFiles", programmes);
        Environment.SetEnvironmentVariable("ProgramW6432", programmes);
        Environment.SetEnvironmentVariable("ProgramFiles(x86)", string.IsNullOrEmpty(programmesX86) ? programmes : programmesX86);
        Environment.SetEnvironmentVariable("ProgramData", communes);
        Environment.SetEnvironmentVariable("ALLUSERSPROFILE", communes);
        Environment.SetEnvironmentVariable("PSModulePath", Path.Combine(systeme, @"WindowsPowerShell\v1.0\Modules"));
    }
#endif

    // Les arguments reçus, remis en une ligne de commande (guillemets et barres obliques inverses selon la règle de Windows)
    static string Arguments(string[] args)
    {
        var sb = new StringBuilder();
        foreach (string a in args)
        {
            if (sb.Length > 0) sb.Append(' ');
            if (a.Length > 0 && a.IndexOfAny(new[] { ' ', '\t', '"' }) < 0) { sb.Append(a); continue; }
            sb.Append('"');
            int barres = 0;
            foreach (char c in a)
            {
                if (c == '\\') { barres++; continue; }
                if (c == '"') { sb.Append('\\', barres * 2 + 1); sb.Append('"'); barres = 0; continue; }
                sb.Append('\\', barres); barres = 0;
                sb.Append(c);
            }
            sb.Append('\\', barres * 2);
            sb.Append('"');
        }
        return sb.ToString();
    }
}
