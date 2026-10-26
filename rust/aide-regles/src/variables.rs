//! Les variables d'environnement que .NET lit au démarrage d'un programme, avant sa première ligne
//! (`$AideVariablesDangereuses`, aide-client.ps1 ; même motif dans Nevermind.ps1). Posées chez l'utilisateur, Nevermind
//! ne fait pas de demande de Windows : l'Aide élevée les recevrait.
//! COR_* et COMPlus_* : .NET Framework ; CORECLR_* et les DOTNET_* qui chargent du code : .NET récent. Les autres
//! DOTNET_* (DOTNET_TieredCompilation...) ne bloquent rien.

/// Ce nom de variable bloque-t-il la demande de Windows ? (sans tenir compte des majuscules, comme Windows)
pub fn dangereuse(nom: &str) -> bool {
    let n = nom.to_ascii_uppercase();
    n.starts_with("COR_")
        || n.starts_with("COMPLUS_")
        || n.starts_with("CORECLR_")
        || n == "DOTNET_STARTUP_HOOKS"
        || n == "DOTNET_ADDITIONAL_DEPS"
        || n == "DOTNET_SHARED_STORE"
}
