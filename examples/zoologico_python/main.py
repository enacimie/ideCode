import sys

from cuidador import Cuidador
from gato import Gato
from perro import Perro
from utilidades import describir, saludar


def main() -> None:
    nombre = sys.argv[1] if len(sys.argv) > 1 else "visitante"
    saludar(nombre)

    cuidador = Cuidador("Dra. Ruiz")
    perro = Perro("Rex", 4, cuidador)
    gato = Gato("Miau", 2)

    cuidador.adoptar(perro)
    cuidador.adoptar(gato)
    cuidador.ronda()

    for animal in cuidador.animales:
        print(describir(animal))

    print(f"Argumentos recibidos: {' '.join(sys.argv[1:])}")


if __name__ == "__main__":
    main()
