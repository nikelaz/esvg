import { XmlDocument } from 'xmldoc';

class SVG {
  private xmlString: string;
  private xmlDoc: XmlDocument;

  constructor(inputSvg: string) {
    this.xmlString = inputSvg;
    this.xmlDoc = new XmlDocument(inputSvg);
  }

  public toString(): string {
    return this.xmlString;
  }

  public getXmlDoc(): XmlDocument {
    return this.xmlDoc;
  }

  public updateFromString(inputString: string): void {
    this.xmlString = inputString;
    this.xmlDoc = new XmlDocument(inputString);
  }

  public updateFromXmlDox(inputXmlDoc: XmlDocument): void {
    this.xmlDoc = inputXmlDoc;
    this.xmlString = this.xmlDoc.toString();
  }
}

export default SVG;
