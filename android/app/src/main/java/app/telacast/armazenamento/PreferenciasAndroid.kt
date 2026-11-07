package app.telacast.armazenamento

import android.content.Context

/** [Preferencias] em SharedPreferences privadas do app. */
class PreferenciasAndroid(contexto: Context) : Preferencias {
    private val p = contexto.getSharedPreferences("telacast_prefs", Context.MODE_PRIVATE)

    override var servidor: String
        get() = p.getString("servidor", "") ?: ""
        set(v) { p.edit().putString("servidor", v).apply() }

    override var usuario: String
        get() = p.getString("usuario", "") ?: ""
        set(v) { p.edit().putString("usuario", v).apply() }

    override var categoriasRecolhidas: Set<Long>
        get() = (p.getStringSet("recolhidas", emptySet()) ?: emptySet()).mapNotNull { it.toLongOrNull() }.toSet()
        set(v) { p.edit().putStringSet("recolhidas", v.map { it.toString() }.toSet()).apply() }

    override var ultimoCanal: Long
        get() = p.getLong("ultimo_canal", 0)
        set(v) { p.edit().putLong("ultimo_canal", v).apply() }

    override var ultimoServidor: Long
        get() = p.getLong("ultimo_servidor", 0)
        set(v) { p.edit().putLong("ultimo_servidor", v).apply() }
}
