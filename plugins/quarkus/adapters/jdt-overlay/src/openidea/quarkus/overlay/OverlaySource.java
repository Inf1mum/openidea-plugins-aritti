package openidea.quarkus.overlay;

import java.io.IOException;
import java.io.StringReader;
import java.util.ArrayList;
import java.util.List;
import java.util.Properties;
import java.util.Set;
import org.eclipse.lsp4mp.jdt.core.project.IConfigSource;
import org.eclipse.lsp4mp.jdt.core.project.MicroProfileConfigPropertyInformation;

/** Immutable editor snapshot replacing, rather than shadowing, one physical source. */
final class OverlaySource implements IConfigSource {
    private final String fileName;
    private final String uri;
    private final int ordinal;
    private final String profile;
    private final boolean env;
    private final Properties values = new Properties();

    OverlaySource(IConfigSource original, String text) throws IOException {
        this(original.getConfigFileName(), original.getSourceConfigFileURI(), original.getOrdinal(), original.getProfile(), false, text);
    }

    /** A source LSP4MP does not know (YAML, .env, config/ or a new unsaved file); text is flattened Properties. */
    OverlaySource(String fileName, String uri, int ordinal, String profile, boolean env, String text) throws IOException {
        this.fileName = fileName;
        this.uri = uri;
        this.ordinal = ordinal;
        this.profile = profile;
        this.env = env;
        Properties parsed = new Properties();
        parsed.load(new StringReader(text));
        parsed.forEach((key, value) -> {
            String name = (String) key;
            if (env || profile == null) values.setProperty(name, (String) value);
            else if (!name.startsWith("%")) values.setProperty("%" + profile + "." + name, (String) value);
        });
    }
    /** Environment-style lookup: %dev.quarkus.http.port is _DEV_QUARKUS_HTTP_PORT, other punctuation becomes _. */
    static String envName(String key) {
        String name = key;
        if (name.startsWith("%")) {
            int dot = name.indexOf('.');
            if (dot > 1) name = "_" + name.substring(1, dot) + "_" + name.substring(dot + 1);
        }
        return name.replaceAll("[^A-Za-z0-9_]", "_").toUpperCase(java.util.Locale.ROOT);
    }
    private String storedName(String key) {
        if (!env) return key;
        String name = envName(key);
        return values.containsKey(name) ? name : null;
    }
    public String getProperty(String key) { String name = storedName(key); return name == null ? null : values.getProperty(name); }
    public Integer getPropertyAsInt(String key) {
        try { String value = getProperty(key); return value == null || value.trim().isEmpty() ? null : Integer.valueOf(value.trim()); }
        catch (NumberFormatException ex) { return null; }
    }
    public String getConfigFileName() { return fileName; }
    public String getSourceConfigFileURI() { return uri; }
    public int getOrdinal() { return ordinal; }
    public String getProfile() { return profile; }
    public Set<String> getAllKeys() { return Set.copyOf(values.stringPropertyNames()); }
    public List<MicroProfileConfigPropertyInformation> getPropertyInformations(String key) {
        List<MicroProfileConfigPropertyInformation> result = new ArrayList<>();
        if (env) {
            String name = storedName(key);
            if (name != null) result.add(new MicroProfileConfigPropertyInformation(key, values.getProperty(name), uri, fileName));
            return result;
        }
        for (String name : getAllKeys()) {
            // The upstream information model cannot represent a malformed %profile key.
            if (name.isEmpty() || (name.startsWith("%") && name.indexOf('.') < 2)) continue;
            if (MicroProfileConfigPropertyInformation.getPropertyNameWithoutProfile(name).equals(key))
                result.add(new MicroProfileConfigPropertyInformation(name, values.getProperty(name),
                    getSourceConfigFileURI(), getConfigFileName()));
        }
        return result;
    }
    public void reset() { /* immutable: next synchronize evicts and replaces this source */ }
}
