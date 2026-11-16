// B5: validazione contro gli schemi ECMA-376 (OOXML transitional) con XmlBeans, gli schemi compilati di Apache POI.
// java -cp "lib/*:." Ooxml file.docx|file.xlsx ...   -> una riga TSV per file: nome, parti controllate, errori di schema (conteggio), primo errore
import java.io.*;
import java.util.*;
import org.apache.poi.openxml4j.opc.*;
import org.apache.xmlbeans.*;
import org.openxmlformats.schemas.wordprocessingml.x2006.main.*;
import org.openxmlformats.schemas.spreadsheetml.x2006.main.*;

public class Ooxml {
    public static void main(String[] a) throws Exception {
        for (String f : a) {
            int parti = 0, errori = 0; String primo = "-";
            try (OPCPackage p = OPCPackage.open(f, PackageAccess.READ)) {
                for (PackagePart part : p.getParts()) {
                    String ct = part.getContentType(), name = part.getPartName().getName();
                    if (ct.contains("core-properties") || ct.contains("relationships")) continue;
                    XmlObject o = null;
                    try (InputStream in = part.getInputStream()) {
                        if (ct.contains("wordprocessingml.document.main")) o = DocumentDocument.Factory.parse(in);
                        else if (ct.contains("wordprocessingml.styles")) o = StylesDocument.Factory.parse(in);
                        else if (ct.contains("wordprocessingml.numbering")) o = NumberingDocument.Factory.parse(in);
                        else if (ct.contains("wordprocessingml.header")) o = HdrDocument.Factory.parse(in);
                        else if (ct.contains("wordprocessingml.footer")) o = FtrDocument.Factory.parse(in);
                        else if (ct.contains("wordprocessingml.footnotes")) o = FootnotesDocument.Factory.parse(in);
                        else if (ct.contains("spreadsheetml.sheet.main")) o = WorkbookDocument.Factory.parse(in);
                        else if (ct.contains("spreadsheetml.worksheet")) o = WorksheetDocument.Factory.parse(in);
                        else if (ct.contains("spreadsheetml.styles")) o = StyleSheetDocument.Factory.parse(in);
                        else if (ct.contains("spreadsheetml.sharedStrings")) o = SstDocument.Factory.parse(in);
                    } catch (XmlException e) { errori++; if (primo.equals("-")) primo = name + ": non è XML valido: " + e.getMessage(); continue; }
                    if (o == null) continue;
                    parti++;
                    List<XmlError> err = new ArrayList<>();
                    XmlOptions opt = new XmlOptions(); opt.setErrorListener(err);
                    if (!o.validate(opt)) { errori += err.size(); if (primo.equals("-")) primo = name + ": " + err.get(0).getMessage(); }
                }
            } catch (Throwable e) { errori = -1; primo = "ERRORE " + e.getClass().getSimpleName() + ": " + e.getMessage(); }
            System.out.println(new File(f).getName() + "\t" + parti + "\t" + errori + "\t" + primo.replace('\t', ' ').replace('\n', ' '));
        }
    }
}
