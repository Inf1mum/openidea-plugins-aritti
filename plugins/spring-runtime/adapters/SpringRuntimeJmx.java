import java.util.*;
import javax.management.*;
import javax.management.openmbean.CompositeData;
import javax.management.remote.*;

/** Read-only JMX probe: localhost only, no credentials, getAttribute/getMBeanInfo only. */
public class SpringRuntimeJmx {
    static final String[][] ALLOWED = {
        {"java.lang:type=Runtime", "Uptime", "VmName", "VmVersion", "InputArguments"},
        {"java.lang:type=Memory", "HeapMemoryUsage"},
        {"java.lang:type=Threading", "ThreadCount"},
        {"java.lang:type=OperatingSystem", "SystemLoadAverage", "AvailableProcessors"},
    };
    static final String[] SPRING = {
        "org.springframework.boot:type=Endpoint,name=Health",
        "org.springframework.boot:type=Endpoint,name=Info",
    };

    static String esc(String s) {
        StringBuilder b = new StringBuilder("\"");
        for (char c : (s.length() > 400 ? s.substring(0, 400) : s).toCharArray()) {
            if (c == '"' || c == '\\') b.append('\\').append(c);
            else if (c < 0x20) b.append(String.format("\\u%04x", (int) c));
            else b.append(c);
        }
        return b.append('"').toString();
    }

    static String value(Object v) {
        if (v instanceof CompositeData d) {
            return "used=" + d.get("used") + " committed=" + d.get("committed") + " max=" + d.get("max");
        }
        if (v instanceof Object[] a) {
            StringBuilder b = new StringBuilder("[");
            for (int i = 0; i < Math.min(a.length, 64); i++) b.append(i > 0 ? "," : "").append(esc(String.valueOf(a[i])));
            return b.append("]").toString();
        }
        return esc(String.valueOf(v));
    }

    static void add(StringBuilder out, MBeanServerConnection c, String bean, String attr) {
        try {
            Object v = c.getAttribute(new ObjectName(bean), attr);
            out.append(out.length() > 1 ? "," : "").append(esc(bean + "#" + attr)).append(':')
               .append(v instanceof Object[] ? value(v) : esc(v instanceof CompositeData ? value(v) : String.valueOf(v)));
        } catch (Exception e) {
        }
    }

    public static void main(String[] args) throws Exception {
        int port = args.length == 1 ? Integer.parseInt(args[0]) : 0;
        if (port < 1024 || port > 65535) {
            System.err.println("port must be 1024-65535");
            System.exit(2);
        }
        JMXServiceURL url = new JMXServiceURL("service:jmx:rmi:///jndi/rmi://localhost:" + port + "/jmxrmi");
        StringBuilder out = new StringBuilder("{");
        try (JMXConnector connector = JMXConnectorFactory.connect(url)) {
            MBeanServerConnection c = connector.getMBeanServerConnection();
            for (String[] entry : ALLOWED) {
                for (int i = 1; i < entry.length; i++) add(out, c, entry[0], entry[i]);
            }
            for (String name : SPRING) {
                ObjectName on = new ObjectName(name);
                if (!c.isRegistered(on)) continue;
                int taken = 0;
                for (MBeanAttributeInfo info : c.getMBeanInfo(on).getAttributes()) {
                    if (info.isReadable() && taken++ < 16) add(out, c, name, info.getName());
                }
            }
        }
        System.out.println(out.append('}'));
    }
}
