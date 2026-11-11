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
    private final IConfigSource original;
    private final Properties values = new Properties();

    OverlaySource(IConfigSource original, String text) throws IOException {
        this.original = original;
        Properties parsed = new Properties();
        parsed.load(new StringReader(text));
        String profile = original.getProfile();
        parsed.forEach((key, value) -> {
            String name = (String) key;
            if (profile == null) values.setProperty(name, (String) value);
            else if (!name.startsWith("%")) values.setProperty("%" + profile + "." + name, (String) value);
        });
    }
    public String getProperty(String key) { return values.getProperty(key); }
    public Integer getPropertyAsInt(String key) {
        try { String value = getProperty(key); return value == null || value.trim().isEmpty() ? null : Integer.valueOf(value.trim()); }
        catch (NumberFormatException ex) { return null; }
    }
    public String getConfigFileName() { return original.getConfigFileName(); }
    public String getSourceConfigFileURI() { return original.getSourceConfigFileURI(); }
    public int getOrdinal() { return original.getOrdinal(); }
    public String getProfile() { return original.getProfile(); }
    public Set<String> getAllKeys() { return Set.copyOf(values.stringPropertyNames()); }
    public List<MicroProfileConfigPropertyInformation> getPropertyInformations(String key) {
        List<MicroProfileConfigPropertyInformation> result = new ArrayList<>();
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
